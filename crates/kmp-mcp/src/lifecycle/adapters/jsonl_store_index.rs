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
        // Written beside and renamed over, never truncated in place: every
        // command that opens a store now rewrites this note, often several
        // processes at once, and a reader that met a half-written file read
        // it as empty and wrote back only its own path — losing every other
        // store the machine had remembered (#903).
        let mut staging = self.index.as_os_str().to_owned();
        staging.push(format!(".{}.tmp", std::process::id()));
        let staging = PathBuf::from(staging);
        let failed = |error: std::io::Error| {
            let _ = std::fs::remove_file(&staging);
            LifecycleError::StoreIndex(format!(
                "could not update store index `{}`: {error}",
                self.index.display()
            ))
        };
        std::fs::write(&staging, Self::body(paths)).map_err(failed)?;
        std::fs::rename(&staging, &self.index).map_err(failed)
    }

    fn erase(&self) -> Result<(), LifecycleError> {
        std::fs::remove_file(&self.index).map_err(|error| {
            LifecycleError::StoreIndex(format!("could not remove empty store index: {error}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rewrite_replaces_the_note_whole_and_leaves_no_staging_file() {
        let base = tempfile::tempdir().expect("temp");
        let index = JsonlStoreIndex::new(base.path());
        index
            .replace(&[PathBuf::from("/a/.kernel"), PathBuf::from("/b/.kernel")])
            .expect("first note");
        index
            .replace(&[PathBuf::from("/a/.kernel")])
            .expect("second note");

        assert_eq!(index.remembered(), Some(vec![PathBuf::from("/a/.kernel")]));
        let names: Vec<_> = std::fs::read_dir(base.path().join("kmp"))
            .expect("kmp dir")
            .flatten()
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(names, [INDEX_FILE], "only the note itself remains");
    }
}
