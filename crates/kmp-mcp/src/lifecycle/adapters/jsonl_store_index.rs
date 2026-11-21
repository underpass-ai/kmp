use std::path::{Path, PathBuf};

use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::store_leases_dir::store_leases_dir;
use crate::lifecycle::ports::store_index::StoreIndex;

/// Where the note lives, under the user data home beside the stores it names.
const INDEX_FILE: &str = "known-stores.jsonl";

/// The machine-local note of project stores, one JSON line per path.
pub struct JsonlStoreIndex {
    index: PathBuf,
    lock: PathBuf,
}

impl JsonlStoreIndex {
    pub fn new(data_home: &Path) -> Self {
        Self {
            index: data_home.join("kmp").join(INDEX_FILE),
            // Beside the store leases: machine-local coordination that a
            // full uninstall removes once nothing holds it, and that a
            // selective one keeps for the next process.
            lock: store_leases_dir(data_home).join("known-stores.lock"),
        }
    }

    fn paths(contents: &str) -> Vec<PathBuf> {
        contents
            .lines()
            .filter_map(|line| {
                serde_json::from_str::<serde_json::Value>(line)
                    .ok()?
                    .get("path")?
                    .as_str()
                    .map(PathBuf::from)
            })
            .collect()
    }

    fn failure(&self, error: &std::io::Error) -> LifecycleError {
        LifecycleError::StoreIndex(format!(
            "could not update store index `{}`: {error}",
            self.index.display()
        ))
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
        Some(Self::paths(&contents))
    }

    fn update(
        &self,
        edit: &mut dyn FnMut(Option<Vec<PathBuf>>) -> Option<Vec<PathBuf>>,
    ) -> Result<(), LifecycleError> {
        let Some(parent) = self.index.parent() else {
            return Ok(());
        };
        std::fs::create_dir_all(parent).map_err(|error| self.failure(&error))?;
        if let Some(leases) = self.lock.parent() {
            std::fs::create_dir_all(leases).map_err(|error| self.failure(&error))?;
        }
        // One writer at a time, across processes: every process that opens
        // a store rewrites this note, and an unlocked read-modify-write let
        // simultaneous openers each drop the others' paths (N8). The lock
        // lives in its own file so the note itself can still be replaced by
        // rename; it is released when `lock` is dropped, or by the system if
        // the process dies.
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&self.lock)
            .map_err(|error| self.failure(&error))?;
        lock.lock().map_err(|error| self.failure(&error))?;
        let current = match std::fs::read_to_string(&self.index) {
            Ok(contents) => Some(Self::paths(&contents)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            // A note that is there and cannot be read is kept as it is:
            // rewriting it from nothing would forget every store it names.
            Err(error) => return Err(self.failure(&error)),
        };
        let Some(paths) = edit(current) else {
            return Ok(());
        };
        if paths.is_empty() {
            return match std::fs::remove_file(&self.index) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    Err(self.failure(&error))
                }
                _ => Ok(()),
            };
        }
        // Written beside and renamed over, never truncated in place, so a
        // reader that takes no lock never meets a half-written note (#903).
        let mut staging = self.index.as_os_str().to_owned();
        staging.push(format!(".{}.tmp", std::process::id()));
        let staging = PathBuf::from(staging);
        let written = std::fs::write(&staging, Self::body(&paths))
            .and_then(|()| std::fs::rename(&staging, &self.index));
        written.map_err(|error| {
            let _ = std::fs::remove_file(&staging);
            self.failure(&error)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(index: &JsonlStoreIndex, paths: &[&str]) {
        let paths = paths.iter().map(PathBuf::from).collect::<Vec<_>>();
        index
            .update(&mut |_| Some(paths.clone()))
            .expect("note rewritten");
    }

    #[test]
    fn a_rewrite_replaces_the_note_whole_and_leaves_no_staging_file() {
        let base = tempfile::tempdir().expect("temp");
        let index = JsonlStoreIndex::new(base.path());
        set(&index, &["/a/.kernel", "/b/.kernel"]);
        set(&index, &["/a/.kernel"]);

        assert_eq!(index.remembered(), Some(vec![PathBuf::from("/a/.kernel")]));
        let mut names: Vec<_> = std::fs::read_dir(base.path().join("kmp"))
            .expect("kmp dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [INDEX_FILE, "store-leases"],
            "only the note, and the lease directory that holds its lock"
        );
    }

    #[test]
    fn an_empty_rewrite_removes_the_note() {
        let base = tempfile::tempdir().expect("temp");
        let index = JsonlStoreIndex::new(base.path());
        set(&index, &["/a/.kernel"]);
        set(&index, &[]);

        assert_eq!(index.remembered(), None);
    }

    #[test]
    fn simultaneous_writers_each_keep_their_path() {
        // N8: twelve openers at once used to leave five to seven of their
        // stores out of the note.
        let base = tempfile::tempdir().expect("temp");
        let data_home = base.path().to_path_buf();
        let writers = (0..16)
            .map(|writer| {
                let data_home = data_home.clone();
                std::thread::spawn(move || {
                    let index = JsonlStoreIndex::new(&data_home);
                    let path = PathBuf::from(format!("/store/{writer}"));
                    index
                        .update(&mut |known| {
                            let mut known = known.unwrap_or_default();
                            known.push(path.clone());
                            Some(known)
                        })
                        .expect("note rewritten");
                })
            })
            .collect::<Vec<_>>();
        for writer in writers {
            writer.join().expect("writer");
        }

        let remembered = JsonlStoreIndex::new(&data_home).remembered().expect("note");
        assert_eq!(remembered.len(), 16, "{remembered:?}");
    }
}
