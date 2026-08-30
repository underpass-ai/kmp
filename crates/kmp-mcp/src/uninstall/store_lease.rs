//! A session's claim on a store.
//!
//! One concept: the file a running host holds open for as long as it is using
//! a store, so another process can tell "in use" from "left behind". The lease
//! is advisory and the guard reads it; nothing here decides what to do about a
//! store that is held.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const STORE_LEASES_DIR: &str = "store-leases";

/// A shared claim held for the lifetime of one embedded MCP host.
///
/// SQLite deliberately lets several hosts share a store. The shared file lock
/// preserves that contract while giving selective uninstall one cross-platform
/// operation that can prove no host still owns the path before removing it.
pub struct StoreSessionLease {
    _file: File,
}

/// Machine-local coordination files live outside the stores they protect.
/// Keeping the lock outside the selected directory lets uninstall hold it
/// until the directory is fully gone, including on Windows.
pub fn store_leases_dir(data_home: &Path) -> PathBuf {
    data_home.join("kmp").join(STORE_LEASES_DIR)
}

pub(in crate::uninstall) fn store_lease_path(data_home: &Path, store: &Path) -> PathBuf {
    let identity = std::fs::canonicalize(store).unwrap_or_else(|_| store.to_path_buf());
    let digest = Sha256::digest(identity.to_string_lossy().as_bytes());
    store_leases_dir(data_home).join(format!("{digest:x}.lock"))
}

pub(in crate::uninstall) fn open_store_lease(
    data_home: &Path,
    store: &Path,
) -> Result<(File, PathBuf), String> {
    let directory = store_leases_dir(data_home);
    std::fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "could not prepare store-use locks in `{}`: {error}",
            directory.display()
        )
    })?;
    let path = store_lease_path(data_home, store);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| {
            format!(
                "could not open store-use lock `{}`: {error}",
                path.display()
            )
        })?;
    Ok((file, path))
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
