use std::path::{Path, PathBuf};

use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::ports::store_index::StoreIndex;

/// Where the note lives, under the user data home beside the stores it names.
const INDEX_FILE: &str = "known-stores.jsonl";

/// The machine-local note of project stores, one JSON line per path.
pub struct JsonlStoreIndex {
    index: PathBuf,
}

impl JsonlStoreIndex {
    pub fn new(data_home: &Path) -> Self {
        Self {
            index: data_home.join("kmp").join(INDEX_FILE),
        }
    }

    fn body(paths: &[PathBuf]) -> String {
        let lines = paths
            .iter()
            .map(|path| {
                format!(
                    "{{\"path\":{}}}",
                    serde_json::json!(path.display().to_string())
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("{lines}\n")
    }
}

impl StoreIndex for JsonlStoreIndex {
    fn location(&self) -> PathBuf {
        self.index.clone()
    }

    fn remembered(&self) -> Option<Vec<PathBuf>> {
        if !self.index.exists() {
            return None;
        }
        let Ok(contents) = std::fs::read_to_string(&self.index) else {
            return Some(Vec::new());
        };
        Some(
            contents
                .lines()
                .filter_map(|line| {
                    serde_json::from_str::<serde_json::Value>(line)
                        .ok()?
                        .get("path")?
                        .as_str()
                        .map(PathBuf::from)
                })
                .collect(),
        )
    }

    fn replace(&self, paths: &[PathBuf]) -> Result<(), LifecycleError> {
        let Some(parent) = self.index.parent() else {
            return Ok(());
        };
        std::fs::create_dir_all(parent).map_err(|error| {
            LifecycleError::StoreIndex(format!(
                "could not update store index `{}`: {error}",
                self.index.display()
            ))
        })?;
        std::fs::write(&self.index, Self::body(paths)).map_err(|error| {
            LifecycleError::StoreIndex(format!(
                "could not update store index `{}`: {error}",
                self.index.display()
            ))
        })
    }

    fn erase(&self) -> Result<(), LifecycleError> {
        std::fs::remove_file(&self.index).map_err(|error| {
            LifecycleError::StoreIndex(format!("could not remove empty store index: {error}"))
        })
    }
}
