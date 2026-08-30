//! How a piece is described to the reader.
//!
//! One concept: the one-line detail that lets someone judge a piece before
//! agreeing to lose it — a size, a store format, an engine's versions, the
//! name a rescue file is given. The survey writes it and the report prints it,
//! so it belongs to neither.

use std::path::Path;

use crate::uninstall::discovery::directory_size;

pub(in crate::uninstall) fn describe_size(path: &Path) -> String {
    let bytes = directory_size(path);
    if bytes >= 1_048_576 {
        format!("{:.1}M", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{}K", bytes / 1_024)
    } else {
        format!("{bytes}B")
    }
}

pub(in crate::uninstall) fn describe_store(store: &Path) -> String {
    let format = std::fs::read_to_string(store.join("FORMAT_VERSION"))
        .map(|text| text.trim().to_string())
        .unwrap_or_else(|_| "?".to_string());
    format!("{} · store format {format}", describe_size(store))
}

pub(in crate::uninstall) fn describe_versions(directory: &Path) -> String {
    let mut versions = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    versions.sort();
    if versions.is_empty() {
        describe_size(directory)
    } else {
        format!("{} ({})", describe_size(directory), versions.join(", "))
    }
}

pub(in crate::uninstall) fn store_label(store: &Path) -> String {
    let name = store
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("store");
    // A project store is always called `.kernel`, so the directory above it is
    // the name a person would recognise.
    if name == ".kernel" {
        return store
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();
    }
    name.to_string()
}
