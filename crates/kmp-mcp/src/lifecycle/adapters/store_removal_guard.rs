use std::path::Path;

use super::lease_claim::LeaseClaim;
use super::lease_claim_error::LeaseClaimError;
use super::store_lease_files::{active_store_message, live_store_holders};
use super::store_lease_registry::claim;

/// The exclusive claim held across export and removal.
pub struct StoreRemovalGuard {
    _claim: LeaseClaim,
}

impl StoreRemovalGuard {
    /// Refuse removal while any current host holds the selected store.
    pub fn acquire(data_home: &Path, store: &Path) -> Result<Self, String> {
        let claim = match claim(data_home, store, true) {
            Ok(claim) => claim,
            Err(LeaseClaimError::Busy) => return Err(active_store_message(store)),
            Err(LeaseClaimError::Io(error)) => {
                return Err(format!("could not exclusively claim the store: {error}"));
            }
        };

        // A pre-fix host does not know about the lease file. Linux exposes
        // its open SQLite descriptor, so protect upgrades from those live
        // sessions as well as sessions started by the corrected binary.
        let holders = live_store_holders(store);
        if !holders.is_empty() {
            return Err(format!(
                "store `{}` is active in {}; stop or restart that owning host and retry. Nothing was removed",
                store.display(),
                holders.join(", ")
            ));
        }
        Ok(Self { _claim: claim })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::StoreRemovalGuard;
    use crate::lifecycle::adapters::store_session_lease::StoreSessionLease;

    fn store_at(path: &Path, format: &str) {
        std::fs::create_dir_all(path.join("store")).expect("store dir");
        std::fs::write(path.join("FORMAT_VERSION"), format).expect("format stamp");
        std::fs::write(path.join("store/kernel.sqlite3"), vec![0u8; 2_048]).expect("store file");
    }

    #[test]
    fn a_live_store_session_blocks_removal_without_blocking_other_stores() {
        let base = tempfile::tempdir().expect("temp");
        let data_home = base.path().join("data");
        let active = base.path().join("active");
        let other = base.path().join("other");
        store_at(&active, "2");
        store_at(&other, "2");

        let session = StoreSessionLease::acquire(&data_home, &active).expect("session lease");
        let refusal = StoreRemovalGuard::acquire(&data_home, &active)
            .err()
            .expect("an active session refuses removal");
        assert!(refusal.contains("active"), "{refusal}");
        assert!(refusal.contains("Nothing was removed"), "{refusal}");

        let unrelated = StoreRemovalGuard::acquire(&data_home, &other)
            .expect("a different store has a different identity");
        drop(unrelated);
        drop(session);
        StoreRemovalGuard::acquire(&data_home, &active)
            .expect("the store becomes removable when its owner exits");
    }

    /// A host that spawns while it holds a claim must not lend the claim to
    /// the child. Record locks are never inherited, so none of these rounds
    /// can meet a lock that only a half-spawned child still holds.
    #[test]
    fn a_child_spawned_by_another_thread_never_keeps_the_store_busy() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let base = tempfile::tempdir().expect("temp");
        let data_home = base.path().join("data");
        let active = base.path().join("active");
        store_at(&active, "2");
        let stop = Arc::new(AtomicBool::new(false));
        let spawner = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let _ = std::process::Command::new("true").status();
                }
            })
        };
        let outcome = (0..50).try_for_each(|round| {
            let session = StoreSessionLease::acquire(&data_home, &active)
                .map_err(|error| format!("round {round}, session: {error}"))?;
            drop(session);
            StoreRemovalGuard::acquire(&data_home, &active)
                .map(drop)
                .map_err(|error| format!("round {round}, removal: {error}"))
        });
        stop.store(true, Ordering::Relaxed);
        spawner.join().expect("spawner");
        outcome.expect("a claim is never lent to a child");
    }
}
