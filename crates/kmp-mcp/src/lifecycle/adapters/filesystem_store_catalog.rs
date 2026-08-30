use std::path::{Path, PathBuf};

use crate::lifecycle::domain::store_facts::StoreFacts;
use crate::lifecycle::domain::store_size::StoreSize;
use crate::lifecycle::domain::store_storage::StoreStorage;
use crate::lifecycle::ports::store_catalog::StoreCatalog;

/// The filesystem's answers about memory stores: a `readdir` filtered on
/// `FORMAT_VERSION` under the user data home, and per-store facts read from
/// the store itself — its stamp, its bytes, its own rotating startup log.
pub struct FilesystemStoreCatalog {
    data_home: PathBuf,
}

impl FilesystemStoreCatalog {
    pub fn new(data_home: &Path) -> Self {
        Self {
            data_home: data_home.to_path_buf(),
        }
    }
}

impl StoreCatalog for FilesystemStoreCatalog {
    fn user_default_store(&self) -> PathBuf {
        self.data_home.join("kmp").join("default")
    }

    fn user_scope_stores(&self) -> Vec<PathBuf> {
        let mut stores = Vec::new();
        if let Ok(entries) = std::fs::read_dir(self.data_home.join("kmp")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.join("FORMAT_VERSION").is_file() {
                    stores.push(path);
                }
            }
        }
        stores
    }

    fn is_store(&self, path: &Path) -> bool {
        path.join("FORMAT_VERSION").is_file()
    }

    fn store_facts(&self, path: &Path) -> StoreFacts {
        StoreFacts {
            storage: storage(path),
            size: StoreSize::new(directory_size(path)),
            last_opened: last_startup(path),
        }
    }
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
}

fn storage(store: &Path) -> Option<StoreStorage> {
    let format = read_trimmed(&store.join("FORMAT_VERSION"));
    if format.as_deref() != Some("2") {
        return Some(match format {
            Some(format) if format.chars().all(|character| character.is_ascii_digit()) => {
                StoreStorage::UnsupportedFormat(Some(format))
            }
            _ => StoreStorage::UnsupportedFormat(None),
        });
    }
    let store_dir = store.join("store");
    if store_dir.join("kernel.sqlite3").is_file() {
        return Some(StoreStorage::Sqlite);
    }
    if std::fs::read_dir(store_dir)
        .is_ok_and(|entries| entries.flatten().any(|entry| entry.path().is_file()))
    {
        return Some(StoreStorage::UnsupportedStorage);
    }
    None
}

/// The newest startup this store recorded, from its own rotating log. The
/// log rolls, so the newest line is not in the newest-named file by
/// construction; take the newest timestamp, not the last file.
fn last_startup(store: &Path) -> Option<String> {
    let mut newest: Option<String> = None;
    for entry in std::fs::read_dir(store.join("logs")).ok()?.flatten() {
        let Ok(contents) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for line in contents.lines() {
            let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let message = event["fields"]["message"].as_str().unwrap_or_default();
            if message != "startup succeeded" && message != "startup failed" {
                continue;
            }
            let Some(when) = event["timestamp"].as_str() else {
                continue;
            };
            let when = when.get(..19).unwrap_or(when).replace('T', " ");
            if newest
                .as_deref()
                .is_none_or(|current| current < when.as_str())
            {
                newest = Some(when);
            }
        }
    }
    newest
}

fn directory_size(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_startup_comes_from_the_stores_own_rotating_log() {
        let base = tempfile::tempdir().expect("temp");
        let store = base.path().join("kmp/default");
        std::fs::create_dir_all(store.join("logs")).expect("logs");
        std::fs::write(store.join("FORMAT_VERSION"), "2").expect("stamp");
        std::fs::write(
            store.join("logs/kmp-mcp.log.2026-08-20"),
            "{\"timestamp\":\"2026-08-20T09:00:00Z\",\"fields\":{\"message\":\"startup succeeded\"}}\n",
        )
        .expect("older log");
        std::fs::write(
            store.join("logs/kmp-mcp.log.2026-08-24"),
            "{\"timestamp\":\"2026-08-24T18:30:00Z\",\"fields\":{\"message\":\"startup succeeded\"}}\n",
        )
        .expect("newer log");

        let catalog = FilesystemStoreCatalog::new(base.path());
        assert_eq!(
            catalog.store_facts(&store).last_opened.as_deref(),
            Some("2026-08-24 18:30:00")
        );
    }
}
