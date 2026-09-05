//! Commit-native memory safety for project-scoped embedded stores.
//!
//! A write is bracketed by a local pending marker and an inter-process lock.
//! Before the store can change, the live event stream must exactly match the
//! committed `.kmp/memory.jsonl` stream. The marker then disappears only after
//! the complete post-write stream is durably published. A stale checkout is
//! therefore rejected before SQLite changes, while a crash or an ambiguous
//! backend failure leaves something `doctor` can name.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use kmp_adapter_embedded::{
    BundleHeader, EmbeddedKernelStore, bundle_excluding_abouts, merge_bundles, verify_bundle,
};
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
    excluded_abouts: Vec<String>,
}

impl CommitNativeBundle {
    /// Project stores have a conventional git path. Explicit and user-default
    /// stores do not: exporting either beside the caller's cwd would recreate
    /// the wrong-directory backup bug under another name.
    pub fn for_resolved(resolved: &ResolvedDataDir) -> Option<Self> {
        Self::for_resolved_excluding_abouts(resolved, Vec::new())
    }

    /// Builds a project bundle that carries authored memory while leaving
    /// release-owned abouts in the machine store only.
    pub fn for_resolved_excluding_abouts(
        resolved: &ResolvedDataDir,
        excluded_abouts: Vec<String>,
    ) -> Option<Self> {
        project_bundle_path(resolved).map(|bundle_path| Self {
            data_dir: resolved.path().to_path_buf(),
            bundle_path,
            excluded_abouts,
        })
    }

    pub fn new(data_dir: impl Into<PathBuf>, bundle_path: impl Into<PathBuf>) -> Self {
        Self::new_excluding_abouts(data_dir, bundle_path, Vec::new())
    }

    pub fn new_excluding_abouts(
        data_dir: impl Into<PathBuf>,
        bundle_path: impl Into<PathBuf>,
        excluded_abouts: Vec<String>,
    ) -> Self {
        Self {
            data_dir: data_dir.into(),
            bundle_path: bundle_path.into(),
            excluded_abouts,
        }
    }

    pub fn path(&self) -> &Path {
        &self.bundle_path
    }

    /// Proves that the live store and committed bundle are the same history,
    /// then marks a write as needing an export before the store can change.
    /// The returned guard holds the inter-process lock through publication.
    pub async fn begin_write(
        &self,
        store: &EmbeddedKernelStore,
    ) -> Result<PendingBundleExport, PortError> {
        let pending_dir = self.data_dir.join(PENDING_EXPORT_DIR);
        fs::create_dir_all(&pending_dir).map_err(|error| {
            PortError::Unavailable(format!(
                "could not create commit-native export marker directory `{}`: {error}",
                pending_dir.display()
            ))
        })?;
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
        publish_lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => PortError::Conflict(format!(
                "another commit-native memory write holds `{}`; retry after it completes",
                lock_path.display()
            )),
            std::fs::TryLockError::Error(error) => PortError::Unavailable(format!(
                "could not lock commit-native export `{}`: {error}",
                lock_path.display()
            )),
        })?;

        let pending = pending_bundle_exports(&self.data_dir);
        if !pending.is_empty() {
            return Err(PortError::Conflict(format!(
                "{} commit-native export marker(s) are still pending in `{}`; reconcile the \
                 canonical bundle explicitly before another memory write",
                pending.len(),
                pending_dir.display()
            )));
        }

        let live_before = self.export_authored_bundle(store).await?;
        let live_header = verify_bundle(&live_before)?;
        let canonical_before = match fs::read_to_string(&self.bundle_path) {
            Ok(bundle) => {
                verify_bundle(&bundle).map_err(|error| {
                    PortError::InvalidState(format!(
                        "committed memory bundle `{}` is invalid: {error}",
                        self.bundle_path.display()
                    ))
                })?;
                let authored_bundle = bundle_excluding_abouts(&bundle, &self.excluded_abouts)?;
                let canonical_header = verify_bundle(&authored_bundle)?;
                // Equal-length histories can still be different branches, so
                // compare their decoded event streams rather than trusting
                // metadata alone. Prefixes are valid bundles but not a safe
                // base for a new project write: Git and SQLite must agree
                // exactly before either can advance.
                merge_bundles(&authored_bundle, &live_before, "commit-native-preflight")?;
                if canonical_header.event_count != live_header.event_count {
                    return Err(PortError::Conflict(format!(
                        "committed memory bundle `{}` has {} events while the live store has {}; \
                         refusing to change SQLite until the two histories are explicitly \
                         reconciled",
                        self.bundle_path.display(),
                        canonical_header.event_count,
                        live_header.event_count
                    )));
                }
                Some(bundle)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if live_header.event_count != 0 {
                    return Err(PortError::Conflict(format!(
                        "live project store has {} events but committed memory bundle `{}` is \
                         missing; export or recover it explicitly before another memory write",
                        live_header.event_count,
                        self.bundle_path.display()
                    )));
                }
                None
            }
            Err(error) => {
                return Err(PortError::Unavailable(format!(
                    "could not read committed memory bundle `{}`: {error}",
                    self.bundle_path.display()
                )));
            }
        };

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
        Ok(PendingBundleExport {
            marker,
            publish_lock,
            canonical_before,
            live_before,
        })
    }

    /// Writes the complete stream after a successful memory mutation. An
    /// identical digest is already current, so an idempotent retry does not
    /// churn the snapshot creation time in git.
    pub async fn publish(
        &self,
        store: &EmbeddedKernelStore,
        pending: &PendingBundleExport,
    ) -> Result<BundleHeader, PortError> {
        let bundle = self.export_authored_bundle(store).await?;
        let header = verify_bundle(&bundle)?;
        merge_bundles(
            &pending.live_before,
            &bundle,
            "commit-native-post-write-check",
        )?;
        let live_before_header = verify_bundle(&pending.live_before)?;
        if header.event_count < live_before_header.event_count {
            return Err(PortError::Conflict(format!(
                "live memory history shrank from {} to {} events during a guarded write",
                live_before_header.event_count, header.event_count
            )));
        }

        let canonical_now = match fs::read_to_string(&self.bundle_path) {
            Ok(bundle) => Some(bundle),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(PortError::Unavailable(format!(
                    "could not re-read committed memory bundle `{}`: {error}",
                    self.bundle_path.display()
                )));
            }
        };
        if canonical_now != pending.canonical_before {
            return Err(PortError::Conflict(format!(
                "committed memory bundle `{}` changed during a guarded write; the pending marker \
                 remains for explicit recovery",
                self.bundle_path.display()
            )));
        }
        write_bundle_atomically(&self.bundle_path, &bundle)?;
        Ok(header)
    }

    async fn export_authored_bundle(
        &self,
        store: &EmbeddedKernelStore,
    ) -> Result<String, PortError> {
        if self.excluded_abouts.is_empty() {
            store.export_bundle().await
        } else {
            store
                .export_bundle_excluding_abouts(&self.excluded_abouts)
                .await
        }
    }
}

/// A marker intentionally has no cleanup in `Drop`: unwinding, a killed
/// process, or an ambiguous backend error are precisely the cases that must
/// remain visible to `doctor`.
pub struct PendingBundleExport {
    marker: PathBuf,
    publish_lock: fs::File,
    canonical_before: Option<String>,
    live_before: String,
}

impl PendingBundleExport {
    pub fn complete(self) -> Result<(), PortError> {
        remove_marker(&self.marker)?;
        self.publish_lock.unlock().map_err(|error| {
            PortError::Unavailable(format!(
                "could not unlock commit-native export after clearing `{}`: {error}",
                self.marker.display()
            ))
        })
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

    #[tokio::test]
    async fn pending_marker_survives_until_the_export_completes() {
        let dir = tempfile::tempdir().expect("dir");
        let kernel = crate::EmbeddedKernel::open(&dir.path().join(".kernel")).expect("kernel");
        let native = CommitNativeBundle::new(
            dir.path().join(".kernel"),
            dir.path().join(".kmp/memory.jsonl"),
        );
        let pending = native.begin_write(kernel.store()).await.expect("marker");
        assert_eq!(pending_bundle_exports(&dir.path().join(".kernel")).len(), 1);

        pending.complete().expect("complete");
        assert!(pending_bundle_exports(&dir.path().join(".kernel")).is_empty());
    }

    #[tokio::test]
    async fn a_concurrent_writer_is_rejected_without_blocking_the_runtime() {
        let dir = tempfile::tempdir().expect("dir");
        let data_dir = dir.path().join(".kernel");
        let kernel = crate::EmbeddedKernel::open(&data_dir).expect("kernel");
        let native = CommitNativeBundle::new(&data_dir, dir.path().join(".kmp/memory.jsonl"));
        let lock_path = data_dir.join(EXPORT_LOCK_FILE);
        let competing_writer = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .expect("lock file");
        competing_writer.lock().expect("competing lock");

        let error = match native.begin_write(kernel.store()).await {
            Ok(_) => panic!("a second writer must fail fast"),
            Err(error) => error,
        };

        assert!(matches!(error, PortError::Conflict(_)));
        assert!(pending_bundle_exports(&data_dir).is_empty());
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
