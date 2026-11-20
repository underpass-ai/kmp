//! Bringing a project store and its committed bundle back together when a
//! session opens it.
//!
//! Two states used to need a human: a `git pull` or a fresh clone left the
//! bundle ahead of the store (reads stale or empty, every write refused),
//! and a write killed between its commit and its publish left a pending
//! marker (every later write refused until two commands were run). Both
//! have one safe answer whenever the histories prove it, and this applies
//! it under the same inter-process lock every guarded write takes:
//!
//! - the store is a strict prefix of the bundle → append the bundle's tail;
//! - the bundle is a strict prefix of the store and a marker says a write
//!   was interrupted → publish the store and clear the marker;
//! - the two are equal → clear any marker, nothing else.
//!
//! A divergence, and a store ahead that no marker accounts for (a bundle
//! deliberately checked out at an older commit, say), are reported and left
//! exactly as they are.

use std::fs::{self, OpenOptions};
use std::path::Path;
use std::time::Duration;

use kmp_adapter_embedded::{EmbeddedKernelStore, HistoryRelation, bundle_excluding_abouts};
use kmp_domain::PortError;

use crate::commit_bundle::{
    CommitNativeBundle, EXPORT_LOCK_FILE, clear_pending_bundle_exports, pending_bundle_exports,
};
use crate::write_bundle_atomically;

/// How long an open waits for a write another session is finishing.
const LOCK_ATTEMPTS: u32 = 20;
const LOCK_PAUSE: Duration = Duration::from_millis(50);

/// What opening a project store did about its committed bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenReconciliation {
    /// Store and bundle hold the same history (or neither holds any).
    Current,
    /// The bundle was ahead; its tail was appended.
    FastForwarded { appended: u64, events: u64 },
    /// A write had been interrupted after it committed; the store was
    /// published to the bundle and the marker cleared.
    Recovered { events: u64 },
    /// The store holds writes no marker accounts for; left alone.
    StoreAhead(HistoryRelation),
    /// Branches; left alone.
    Diverged(HistoryRelation),
    /// The store holds events and no bundle exists; left alone.
    BundleMissing { store_events: u64 },
    /// Another session held the lock throughout; nothing was checked.
    Busy,
}

impl CommitNativeBundle {
    /// Compares the store with the committed bundle and applies the one safe
    /// answer, if there is one. Blocking: call it where the store is opened.
    pub fn reconcile_on_open(
        &self,
        store: &EmbeddedKernelStore,
    ) -> Result<OpenReconciliation, PortError> {
        let lock_path = self.data_dir.join(EXPORT_LOCK_FILE);
        let lock = OpenOptions::new()
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
        if !take_lock(&lock, &lock_path)? {
            return Ok(OpenReconciliation::Busy);
        }
        let outcome = self.reconcile_locked(store);
        let _ = lock.unlock();
        outcome
    }

    pub(crate) fn reconcile_locked(
        &self,
        store: &EmbeddedKernelStore,
    ) -> Result<OpenReconciliation, PortError> {
        let pending = !pending_bundle_exports(&self.data_dir).is_empty();
        let committed = match fs::read_to_string(&self.bundle_path) {
            Ok(bundle) => bundle,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let live = self.live_authored(store)?;
                let store_events = kmp_adapter_embedded::verify_bundle(&live)?.event_count;
                if store_events == 0 {
                    clear_pending_bundle_exports(&self.data_dir)?;
                    return Ok(OpenReconciliation::Current);
                }
                // The first write of a project committed and stopped before
                // it published: a missing bundle is an empty history, a
                // prefix of anything.
                if pending {
                    return self.publish_and_clear(&live, store_events);
                }
                return Ok(OpenReconciliation::BundleMissing { store_events });
            }
            Err(error) => {
                return Err(PortError::Unavailable(format!(
                    "could not read committed memory bundle `{}`: {error}",
                    self.bundle_path.display()
                )));
            }
        };
        kmp_adapter_embedded::verify_bundle(&committed).map_err(|error| {
            PortError::InvalidState(format!(
                "committed memory bundle `{}` is invalid: {error}",
                self.bundle_path.display()
            ))
        })?;
        let authored = bundle_excluding_abouts(&committed, &self.excluded_abouts)?;
        let relation = store.fast_forward_to_bundle_blocking(
            &authored,
            &self.excluded_abouts,
            kmp_application::projection_mutations_for_context_event,
        )?;
        match relation {
            HistoryRelation::Equal { .. } => {
                clear_pending_bundle_exports(&self.data_dir)?;
                Ok(OpenReconciliation::Current)
            }
            // Every event the store held is in the bundle, so whatever an
            // interrupted write left in SQLite is committed too.
            HistoryRelation::BundleAhead {
                store_events,
                bundle_events,
            } => {
                clear_pending_bundle_exports(&self.data_dir)?;
                self.published.set(None);
                Ok(OpenReconciliation::FastForwarded {
                    appended: bundle_events - store_events,
                    events: bundle_events,
                })
            }
            HistoryRelation::StoreAhead { store_events, .. } if pending => {
                let live = self.live_authored(store)?;
                self.publish_and_clear(&live, store_events)
            }
            HistoryRelation::StoreAhead { .. } => Ok(OpenReconciliation::StoreAhead(relation)),
            HistoryRelation::Diverged { .. } => Ok(OpenReconciliation::Diverged(relation)),
        }
    }

    fn publish_and_clear(&self, live: &str, events: u64) -> Result<OpenReconciliation, PortError> {
        write_bundle_atomically(&self.bundle_path, live)?;
        self.published.set(None);
        clear_pending_bundle_exports(&self.data_dir)?;
        Ok(OpenReconciliation::Recovered { events })
    }

    fn live_authored(&self, store: &EmbeddedKernelStore) -> Result<String, PortError> {
        if self.excluded_abouts.is_empty() {
            store.export_bundle_blocking()
        } else {
            store.export_bundle_excluding_abouts_blocking(&self.excluded_abouts)
        }
    }
}

fn take_lock(lock: &fs::File, path: &Path) -> Result<bool, PortError> {
    for attempt in 0..LOCK_ATTEMPTS {
        match lock.try_lock() {
            Ok(()) => return Ok(true),
            Err(std::fs::TryLockError::WouldBlock) => {
                if attempt + 1 < LOCK_ATTEMPTS {
                    std::thread::sleep(LOCK_PAUSE);
                }
            }
            Err(std::fs::TryLockError::Error(error)) => {
                return Err(PortError::Unavailable(format!(
                    "could not lock commit-native export `{}`: {error}",
                    path.display()
                )));
            }
        }
    }
    Ok(false)
}
