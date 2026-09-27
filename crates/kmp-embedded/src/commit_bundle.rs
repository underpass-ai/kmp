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

use kmp_adapter_embedded::{
    BundleHeader, EmbeddedKernelStore, bundle_excluding_abouts, merge_bundles, verify_bundle,
};
use kmp_domain::PortError;

use crate::bundle_file::{sync_parent, unique_name};
pub use crate::bundle_file::{write_bundle_atomically, write_bundle_if_absent};
use crate::committed_tail::CommittedTail;
use crate::read_head::ReadHead;
use crate::tail_memo::TailMemo;
use crate::{ResolvedDataDir, project_bundle_path};

pub const PENDING_EXPORT_DIR: &str = "bundle-export-pending";
const EXPORT_LOCK_FILE: &str = "commit-native-bundle.lock";

/// The committed head bundle paired with the machine store it protects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitNativeBundle {
    data_dir: PathBuf,
    bundle_path: PathBuf,
    excluded_abouts: Vec<String>,
    /// This process's last publish: while the committed file and the store
    /// stand where it left them, the next write is checked and published
    /// from their tails (DESIGN L6, write in O(delta)).
    published: TailMemo,
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
            published: TailMemo::default(),
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
            published: TailMemo::default(),
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

        // The store and the committed file where this process's last publish
        // left them are the history it proved equal then: nothing to export.
        let (tail, canonical_before, live_before) = match self.published.take() {
            Some(tail) if tail.holds(store, &self.bundle_path).await? => {
                (Some(tail), None, String::new())
            }
            _ => {
                let (canonical_before, live_before) = self.prove_same_history(store).await?;
                (None, canonical_before, live_before)
            }
        };
        let marker = self.mark_pending(&pending_dir)?;
        Ok(PendingBundleExport {
            marker,
            publish_lock,
            canonical_before,
            live_before,
            tail: std::sync::Mutex::new(tail),
        })
    }

    /// The full check: the live store and the committed bundle hold the same
    /// event stream, compared event by event.
    async fn prove_same_history(
        &self,
        store: &EmbeddedKernelStore,
    ) -> Result<(Option<String>, String), PortError> {
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
        Ok((canonical_before, live_before))
    }

    fn mark_pending(&self, pending_dir: &Path) -> Result<PathBuf, PortError> {
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
        sync_parent(Some(pending_dir))?;
        Ok(marker)
    }

    /// Writes the complete stream after a successful memory mutation. An
    /// identical digest is already current, so an idempotent retry does not
    /// churn the snapshot creation time in git.
    pub async fn publish(
        &self,
        store: &EmbeddedKernelStore,
        pending: &PendingBundleExport,
    ) -> Result<BundleHeader, PortError> {
        let tail = pending
            .tail
            .lock()
            .map_err(|_| PortError::Unavailable("commit-native tail lock poisoned".into()))?
            .take();
        if let Some(tail) = tail {
            return self.publish_from(store, tail).await;
        }
        let head = ReadHead::read(store, &self.excluded_abouts).await?;
        let (bundle, _) = head.bundle()?;
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
        self.published
            .set(CommittedTail::published(head, &self.bundle_path));
        Ok(header)
    }

    /// Publishes from the tail the write began on: the bundle this process
    /// wrote, extended by the events the log holds after it.
    async fn publish_from(
        &self,
        store: &EmbeddedKernelStore,
        tail: CommittedTail,
    ) -> Result<BundleHeader, PortError> {
        if !tail.file_is_ours(&self.bundle_path)? {
            return Err(PortError::Conflict(format!(
                "committed memory bundle `{}` changed during a guarded write; the pending marker \
                 remains for explicit recovery",
                self.bundle_path.display()
            )));
        }
        let Some(head) = tail.extended(store, &self.excluded_abouts).await? else {
            return Err(PortError::Conflict(
                "live memory history changed under a guarded write".to_string(),
            ));
        };
        let (bundle, header) = head.bundle()?;
        write_bundle_atomically(&self.bundle_path, &bundle)?;
        self.published
            .set(CommittedTail::published(head, &self.bundle_path));
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
    /// The tail the write began on, when the full check was not needed.
    tail: std::sync::Mutex<Option<CommittedTail>>,
}

impl PendingBundleExport {
    /// Whether the write began on this process's last publish rather than
    /// on the full check.
    #[cfg(test)]
    pub(crate) fn began_on_the_tail(&self) -> bool {
        self.tail.lock().map(|tail| tail.is_some()).unwrap_or(false)
    }

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
}
