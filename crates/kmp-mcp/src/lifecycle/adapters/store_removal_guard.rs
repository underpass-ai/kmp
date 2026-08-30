use std::fs::{File, TryLockError};
use std::path::Path;

use super::store_lease_files::{active_store_message, live_store_holders, open_store_lease};

/// The exclusive claim held across export and removal.
pub struct StoreRemovalGuard {
    _file: File,
}

impl StoreRemovalGuard {
    /// Refuse removal while any current host holds the selected store.
    pub fn acquire(data_home: &Path, store: &Path) -> Result<Self, String> {
        let (file, path) = open_store_lease(data_home, store)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(active_store_message(data_home, store));
            }
            Err(TryLockError::Error(error)) => {
                return Err(format!(
                    "could not exclusively claim store-use lock `{}`: {error}",
                    path.display()
                ));
            }
        }

        // A pre-fix host does not know about the lease file. Linux exposes
        // its open SQLite descriptor, so protect upgrades from those live
        // sessions as well as sessions started by the corrected binary.
        let holders = live_store_holders(store, &path);
        if !holders.is_empty() {
            return Err(format!(
                "store `{}` is active in {}; stop or restart that owning host and retry. Nothing was removed",
                store.display(),
                holders.join(", ")
            ));
        }
        Ok(Self { _file: file })
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
}
