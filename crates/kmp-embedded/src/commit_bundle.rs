//! Commit-native memory safety for project-scoped embedded stores.
//!
//! A write is bracketed by a local pending marker. The marker exists before
//! the store can change and disappears only after the complete event stream is
//! durably replaced at `.kmp/memory.jsonl`. A crash or an ambiguous backend
//! failure therefore leaves something `doctor` can name instead of silently
//! leaving the only current copy in `.kernel/`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use kmp_adapter_embedded::{BundleHeader, EmbeddedKernelStore, merge_bundles, verify_bundle};
use kmp_domain::PortError;

use crate::{ResolvedDataDir, project_bundle_path};

pub const PENDING_EXPORT_DIR: &str = "bundle-export-pending";
const EXPORT_LOCK_FILE: &str = "commit-native-bundle.lock";

static UNIQUE_FILE: AtomicU64 = AtomicU64::new(0);

/// The committed head bundle paired with the machine store it protects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitNativeBundle {
    data_dir: PathBuf,
    bundle_path: PathBuf,
}

impl CommitNativeBundle {
    /// Project stores have a conventional git path. Explicit and user-default
    /// stores do not: exporting either beside the caller's cwd would recreate
    /// the wrong-directory backup bug under another name.
    pub fn for_resolved(resolved: &ResolvedDataDir) -> Option<Self> {
        project_bundle_path(resolved).map(|bundle_path| Self {
            data_dir: resolved.path().to_path_buf(),
            bundle_path,
        })
    }

    pub fn new(data_dir: impl Into<PathBuf>, bundle_path: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            bundle_path: bundle_path.into(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.bundle_path
    }

    /// Marks a write as needing an export before the store can change.
    pub fn begin_write(&self) -> Result<PendingBundleExport, PortError> {
        let pending_dir = self.data_dir.join(PENDING_EXPORT_DIR);
        fs::create_dir_all(&pending_dir).map_err(|error| {
            PortError::Unavailable(format!(
                "could not create commit-native export marker directory `{}`: {error}",
                pending_dir.display()
            ))
        })?;
        let marker = pending_dir.join(unique_name("write", "pending"));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&marker)
            .map_err(|error| {
                PortError::Unavailable(format!(
                    "could not create commit-native export marker `{}`: {error}",
                    marker.display()
                ))
            })?;
        writeln!(file, "bundle={}", self.bundle_path.display()).map_err(|error| {
            PortError::Unavailable(format!(
                "could not write commit-native export marker `{}`: {error}",
                marker.display()
            ))
        })?;
        file.sync_all().map_err(|error| {
            PortError::Unavailable(format!(
                "could not make commit-native export marker `{}` durable: {error}",
                marker.display()
            ))
        })?;
        sync_parent(Some(&pending_dir))?;
        Ok(PendingBundleExport { marker })
    }

    /// Writes the complete stream after a successful memory mutation. An
    /// identical digest is already current, so an idempotent retry does not
    /// churn the snapshot creation time in git.
    pub async fn publish(&self, store: &EmbeddedKernelStore) -> Result<BundleHeader, PortError> {
        let bundle = store.export_bundle().await?;
        let header = verify_bundle(&bundle)?;
        let lock_path = self.data_dir.join(EXPORT_LOCK_FILE);
        let publish_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|error| {
                PortError::Unavailable(format!(
                    "could not open commit-native export lock `{}`: {error}",
                    lock_path.display()
                ))
            })?;
        publish_lock.lock().map_err(|error| {
            PortError::Unavailable(format!(
                "could not lock commit-native export `{}`: {error}",
                lock_path.display()
            ))
        })?;
        if let Ok(current_bundle) = fs::read_to_string(&self.bundle_path)
            && let Ok(current_header) = verify_bundle(&current_bundle)
        {
            if current_header.content_digest == header.content_digest
                && current_header.event_count == header.event_count
            {
                return Ok(header);
            }
            // This is both a compatibility proof and a stale-writer guard.
            // A later export may already protect more events; never replace
            // it with this writer's shorter view. Divergence is loud and
            // leaves the pending marker for recovery.
            merge_bundles(&current_bundle, &bundle, "commit-native-prefix-check")?;
            if current_header.event_count > header.event_count {
                return Ok(current_header);
            }
        }
        write_bundle_atomically(&self.bundle_path, &bundle)?;
        Ok(header)
    }
}

/// A marker intentionally has no cleanup in `Drop`: unwinding, a killed
/// process, or an ambiguous backend error are precisely the cases that must
/// remain visible to `doctor`.
pub struct PendingBundleExport {
    marker: PathBuf,
}

impl PendingBundleExport {
    pub fn complete(self) -> Result<(), PortError> {
        remove_marker(&self.marker)
    }
}

pub fn pending_bundle_exports(data_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(data_dir.join(PENDING_EXPORT_DIR)) else {
        return Vec::new();
    };
    let mut pending: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    pending.sort();
    pending
}

/// Clears markers after an operator has stopped other writers and explicitly
/// acknowledged that a successful full export contains every committed write.
pub fn clear_pending_bundle_exports(data_dir: &Path) -> Result<(), PortError> {
    for marker in pending_bundle_exports(data_dir) {
        remove_marker(&marker)?;
    }
    Ok(())
}

/// Same-directory durable replacement, so a failed export leaves either the
/// previous complete bundle or the next complete bundle, never half a JSONL
/// stream. Unix rename replaces atomically; Windows keeps the previous file
/// beside it until the new one has taken the canonical name.
pub fn write_bundle_atomically(path: &Path, bundle: &str) -> Result<(), PortError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent).map_err(|error| {
            PortError::Unavailable(format!(
                "could not create bundle directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }
    let temp = path.with_file_name(unique_name("memory", "tmp"));
    let write_result = (|| -> Result<(), PortError> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|error| {
                PortError::Unavailable(format!(
                    "could not create temporary bundle `{}`: {error}",
                    temp.display()
                ))
            })?;
        file.write_all(bundle.as_bytes()).map_err(|error| {
            PortError::Unavailable(format!(
                "could not write temporary bundle `{}`: {error}",
                temp.display()
            ))
        })?;
        file.sync_all().map_err(|error| {
            PortError::Unavailable(format!(
                "could not make temporary bundle `{}` durable: {error}",
                temp.display()
            ))
        })?;
        replace_file(&temp, path).map_err(|error| {
            PortError::Unavailable(format!(
                "could not replace bundle `{}`: {error}",
                path.display()
            ))
        })?;
        sync_parent(parent)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    write_result
}

/// Publishes an immutable bundle without a check-then-replace race. The hard
/// link is an atomic create-if-absent operation on the same filesystem: two
/// snapshot creators can agree on existing content, but neither can replace
/// the other's recovery point.
pub fn write_bundle_if_absent(path: &Path, bundle: &str) -> Result<bool, PortError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent).map_err(|error| {
            PortError::Unavailable(format!(
                "could not create bundle directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }
    let staged = path.with_file_name(unique_name("snapshot", "tmp"));
    write_bundle_atomically(&staged, bundle)?;
    let linked = match fs::hard_link(&staged, path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(error) => {
            let _ = fs::remove_file(&staged);
            return Err(PortError::Unavailable(format!(
                "could not publish immutable bundle `{}`: {error}",
                path.display()
            )));
        }
    };
    fs::remove_file(&staged).map_err(|error| {
        PortError::Unavailable(format!(
            "could not remove staged bundle `{}`: {error}",
            staged.display()
        ))
    })?;
    sync_parent(parent)?;
    Ok(linked)
}

fn remove_marker(marker: &Path) -> Result<(), PortError> {
    match fs::remove_file(marker) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(PortError::Unavailable(format!(
                "could not clear commit-native export marker `{}`: {error}",
                marker.display()
            )));
        }
    }
    if let Some(parent) = marker.parent() {
        // Keep the empty marker directory. Syncing it makes the deletion as
        // durable as creation; removing it immediately would require syncing
        // its parent as a second filesystem transaction.
        sync_parent(Some(parent))?;
    }
    Ok(())
}

fn unique_name(prefix: &str, suffix: &str) -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos();
    let sequence = UNIQUE_FILE.fetch_add(1, Ordering::Relaxed);
    format!(
        ".{prefix}-{}-{time}-{sequence}.{suffix}",
        std::process::id()
    )
}

#[cfg(not(windows))]
fn replace_file(temp: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(temp, destination)
}

#[cfg(windows)]
fn replace_file(temp: &Path, destination: &Path) -> std::io::Result<()> {
    let previous = destination.with_file_name(unique_name("memory", "previous"));
    if destination.exists() {
        fs::rename(destination, &previous)?;
    }
    match fs::rename(temp, destination) {
        Ok(()) => {
            let _ = fs::remove_file(previous);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(previous, destination);
            Err(error)
        }
    }
}

#[cfg(unix)]
fn sync_parent(parent: Option<&Path>) -> Result<(), PortError> {
    let Some(parent) = parent else {
        return Ok(());
    };
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            PortError::Unavailable(format!(
                "could not make bundle directory `{}` durable: {error}",
                parent.display()
            ))
        })
}

#[cfg(not(unix))]
fn sync_parent(_parent: Option<&Path>) -> Result<(), PortError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_marker_survives_until_the_export_completes() {
        let dir = tempfile::tempdir().expect("dir");
        let native = CommitNativeBundle::new(
            dir.path().join(".kernel"),
            dir.path().join(".kmp/memory.jsonl"),
        );
        let pending = native.begin_write().expect("marker");
        assert_eq!(pending_bundle_exports(&dir.path().join(".kernel")).len(), 1);

        pending.complete().expect("complete");
        assert!(pending_bundle_exports(&dir.path().join(".kernel")).is_empty());
    }

    #[test]
    fn atomic_write_replaces_a_complete_bundle() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(".kmp/memory.jsonl");
        write_bundle_atomically(&path, "first\n").expect("first");
        write_bundle_atomically(&path, "second\n").expect("second");
        assert_eq!(fs::read_to_string(path).expect("read"), "second\n");
    }

    #[test]
    fn immutable_write_never_replaces_an_existing_recovery_point() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(".kmp/snapshots/release.jsonl");
        assert!(write_bundle_if_absent(&path, "first\n").expect("created"));
        assert!(!write_bundle_if_absent(&path, "second\n").expect("exists"));
        assert_eq!(fs::read_to_string(path).expect("read"), "first\n");
    }
}
