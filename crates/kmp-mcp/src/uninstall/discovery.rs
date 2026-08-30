//! Where the installer put things.
//!
//! One concept: the filesystem knowledge — engine file names, store layouts,
//! bundles beside a store, the prompts KMP owns inside a directory it shares.
//! It answers what exists; it never decides what happens to it.

use std::path::{Path, PathBuf};

use crate::uninstall::roots::Roots;

pub(in crate::uninstall) fn engine_file_name() -> &'static str {
    if cfg!(windows) {
        "kmp-mcp.exe"
    } else {
        "kmp-mcp"
    }
}

pub(in crate::uninstall) fn engine_directories(roots: &Roots) -> Vec<PathBuf> {
    let mut directories = vec![
        roots.home.join(".local/bin"),
        roots.home.join(".cargo/bin"),
        roots.data_home.join("kmp/bin"),
    ];
    directories.extend(roots.path_entries.iter().cloned());
    directories.sort();
    directories.dedup();
    directories
}

/// Every store this machine has, not only the one this directory resolves to.
/// The per-user default and the project store are both easy to forget, and
/// forgetting one is how an uninstall leaves memory behind.
pub(in crate::uninstall) fn stores(roots: &Roots) -> Vec<PathBuf> {
    // The same enumeration `info` shows, so the dry run cannot promise to
    // remove a set the operator was never shown — including the stores no
    // resolution rule reaches, which are the strongest candidates for removal
    // and the ones nothing would otherwise mention.
    let index = roots
        .data_home
        .join("kmp")
        .join(crate::memories::INDEX_FILE);
    let mut found: Vec<PathBuf> =
        crate::memories::list(&roots.data_home, &crate::memories::read_index(&index))
            .into_iter()
            .map(|memory| memory.path)
            .collect();

    // Plus the one under this directory, which may never have been opened.
    let project = roots.working_dir.join(".kernel");
    if project.join("FORMAT_VERSION").is_file() {
        found.push(project);
    }
    found.sort();
    found.dedup();
    found
}

/// Only a project store has a bundle. An explicit data dir or the per-user
/// default belongs to no repository, so there is no conventional place a copy
/// of it would be — the same reason `export` refuses to guess one.
pub(in crate::uninstall) fn bundle_beside(store: &Path) -> Option<PathBuf> {
    if store.file_name()? != std::ffi::OsStr::new(".kernel") {
        return None;
    }
    store
        .parent()
        .map(|root| root.join(kmp_embedded::PROJECT_BUNDLE_PATH))
}

/// The header of a bundle says how many events it holds, and reading it never
/// opens the store.
pub(in crate::uninstall) fn bundle_event_count(bundle: &Path) -> Option<u64> {
    let contents = std::fs::read_to_string(bundle).ok()?;
    let header = contents.lines().next()?;
    serde_json::from_str::<serde_json::Value>(header)
        .ok()?
        .get("event_count")?
        .as_u64()
}

pub(in crate::uninstall) fn kmp_prompts(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut prompts = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("kmp-") && name.ends_with(".md"))
        })
        .collect::<Vec<_>>();
    prompts.sort();
    prompts
}

pub(in crate::uninstall) fn file_mentions(path: &Path, needle: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|contents| contents.contains(needle))
}

pub(in crate::uninstall) fn directory_size(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    // A symlink is not walked into. Its target may be anywhere, including an
    // ancestor of this walk, and a size that recurses forever is not a size.
    if metadata.is_symlink() {
        return 0;
    }
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| directory_size(&entry.path()))
        .sum()
}
