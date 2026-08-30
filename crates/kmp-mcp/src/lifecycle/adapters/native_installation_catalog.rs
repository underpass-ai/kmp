use std::path::{Path, PathBuf};

use crate::lifecycle::domain::store_size::StoreSize;
use crate::lifecycle::ports::installation_catalog::InstallationCatalog;

/// The real filesystem's answers about an installation.
pub struct NativeInstallationCatalog;

impl InstallationCatalog for NativeInstallationCatalog {
    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_directory(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, String> {
        std::fs::canonicalize(path).map_err(|error| error.to_string())
    }

    fn size_of(&self, path: &Path) -> StoreSize {
        StoreSize::new(directory_size(path))
    }

    fn store_stamp(&self, store: &Path) -> Option<String> {
        std::fs::read_to_string(store.join("FORMAT_VERSION"))
            .ok()
            .map(|text| text.trim().to_string())
    }

    fn entry_names(&self, directory: &Path) -> Vec<String> {
        let mut names = std::fs::read_dir(directory)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    fn files_in(&self, directory: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return Vec::new();
        };
        let mut files = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        files.sort();
        files
    }

    fn file_mentions(&self, path: &Path, needle: &str) -> bool {
        std::fs::read_to_string(path).is_ok_and(|contents| contents.contains(needle))
    }

    fn bundle_event_count(&self, bundle: &Path) -> Option<u64> {
        let contents = std::fs::read_to_string(bundle).ok()?;
        let header = contents.lines().next()?;
        serde_json::from_str::<serde_json::Value>(header)
            .ok()?
            .get("event_count")?
            .as_u64()
    }

    fn remove_path(&self, path: &Path) -> Result<(), String> {
        let result = if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        result.map_err(|error| format!("could not remove `{}`: {error}", path.display()))
    }
}

fn directory_size(path: &Path) -> u64 {
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
