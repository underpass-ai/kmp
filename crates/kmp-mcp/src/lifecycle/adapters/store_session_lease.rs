use std::fs::{File, TryLockError};
use std::path::Path;

use super::store_lease_files::open_store_lease;

/// A shared claim held for the lifetime of one embedded MCP host.
///
/// SQLite deliberately lets several hosts share a store. The shared file lock
/// preserves that contract while giving selective uninstall one
/// cross-platform operation that can prove no host still owns the path
/// before removing it.
pub struct StoreSessionLease {
    _file: File,
}

impl StoreSessionLease {
    /// Claim a store for this host without excluding other hosts.
    pub fn acquire(data_home: &Path, store: &Path) -> Result<Self, String> {
        let (file, path) = open_store_lease(data_home, store)?;
        file.try_lock_shared().map_err(|error| match error {
            TryLockError::WouldBlock => format!(
                "store `{}` is being removed; this host did not open it. Retry after uninstall finishes",
                store.display()
            ),
            TryLockError::Error(error) => {
                format!("could not claim store-use lock `{}`: {error}", path.display())
            }
        })?;
        Ok(Self { _file: file })
    }
}
