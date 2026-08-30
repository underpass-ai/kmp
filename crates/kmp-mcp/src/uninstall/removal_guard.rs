//! Refusing to remove a store somebody is holding.
//!
//! One concept: proving a store is free before anything touches it, and saying
//! who has it when it is not. `live_store_holders` is the only part that reads
//! the operating system, and on anything but Linux it answers "nobody" rather
//! than guessing — the lease itself still refuses.

use std::fs::{File, TryLockError};
use std::path::Path;

use crate::uninstall::store_lease::open_store_lease;
use crate::uninstall::store_lease::store_lease_path;

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

        // A pre-fix host does not know about the lease file. Linux exposes its
        // open SQLite descriptor, so protect upgrades from those live sessions
        // as well as sessions started by the corrected binary.
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

fn active_store_message(data_home: &Path, store: &Path) -> String {
    let holders = live_store_holders(store, &store_lease_path(data_home, store));
    let owner = if holders.is_empty() {
        "another KMP host".to_string()
    } else {
        holders.join(", ")
    };
    format!(
        "store `{}` is active in {owner}; stop or restart that owning host and retry. Nothing was removed",
        store.display()
    )
}

#[cfg(target_os = "linux")]
fn live_store_holders(store: &Path, lease: &Path) -> Vec<String> {
    let store = std::fs::canonicalize(store).unwrap_or_else(|_| store.to_path_buf());
    let lease = std::fs::canonicalize(lease).unwrap_or_else(|_| lease.to_path_buf());
    let own_pid = std::process::id();
    let Ok(processes) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut holders = Vec::new();
    for process in processes.flatten() {
        let Some(pid) = process
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == own_pid {
            continue;
        }
        let holds_path = std::fs::read_dir(process.path().join("fd"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|fd| std::fs::read_link(fd.path()).ok())
            .any(|open| open == lease || open.starts_with(&store));
        if !holds_path {
            continue;
        }
        let command = std::fs::read(process.path().join("cmdline"))
            .ok()
            .map(|bytes| {
                bytes
                    .split(|byte| *byte == 0)
                    .filter(|part| !part.is_empty())
                    .map(String::from_utf8_lossy)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|command| !command.is_empty())
            .unwrap_or_else(|| "unknown command".to_string());
        holders.push(format!("pid {pid} (`{command}`)"));
    }
    holders.sort();
    holders
}

#[cfg(not(target_os = "linux"))]
fn live_store_holders(_store: &Path, _lease: &Path) -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uninstall::store_lease::StoreSessionLease;
    use crate::uninstall::test_support::store_at;

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
