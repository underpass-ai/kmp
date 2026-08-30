//! Temporary installations to survey.
//!
//! One concept: building a machine on disk that looks installed, so a verb
//! whose whole job is deletion can be exercised without deleting anything
//! real. Taken as roots rather than read from the environment is what makes
//! that possible at all.

#![cfg(test)]

use std::path::Path;

use crate::uninstall::roots::Roots;

pub(in crate::uninstall) fn roots(base: &Path) -> Roots {
    Roots {
        home: base.join("home"),
        data_home: base.join("home/.local/share"),
        working_dir: base.join("project"),
        path_entries: Vec::new(),
    }
}

pub(in crate::uninstall) fn store_at(path: &Path, format: &str) {
    std::fs::create_dir_all(path.join("store")).expect("store dir");
    std::fs::write(path.join("FORMAT_VERSION"), format).expect("format stamp");
    std::fs::write(path.join("store/kernel.sqlite3"), vec![0u8; 2_048]).expect("store file");
}

pub(in crate::uninstall) fn bundle_at(path: &Path, events: u64) {
    std::fs::create_dir_all(path.parent().expect("bundle parent")).expect("bundle dir");
    std::fs::write(path, format!("{{\"event_count\":{events}}}\n")).expect("bundle");
}
