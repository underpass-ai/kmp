//! The shared mechanics under both store-lease adapters: where a store's
//! lock file lives, how it is opened, and who is holding the store right
//! now. No policy — the two lease types decide what a claim means.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::lifecycle::domain::store_leases_dir::store_leases_dir;

pub(crate) fn store_lease_path(data_home: &Path, store: &Path) -> PathBuf {
    let identity = std::fs::canonicalize(store).unwrap_or_else(|_| store.to_path_buf());
    let digest = Sha256::digest(identity.to_string_lossy().as_bytes());
    store_leases_dir(data_home).join(format!("{digest:x}.lock"))
}

pub(crate) fn open_store_lease(data_home: &Path, store: &Path) -> Result<(File, PathBuf), String> {
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

pub(crate) fn active_store_message(data_home: &Path, store: &Path) -> String {
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
pub(crate) fn live_store_holders(store: &Path, lease: &Path) -> Vec<String> {
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
pub(crate) fn live_store_holders(_store: &Path, _lease: &Path) -> Vec<String> {
    Vec::new()
}
