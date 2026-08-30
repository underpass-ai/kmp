use std::path::Path;

use crate::lifecycle::ports::store_catalog::StoreCatalog;
use crate::lifecycle::ports::store_index::StoreIndex;

/// Use case: remember that a data directory exists, so it can be listed from
/// anywhere later. Appends only when the path is new to the index.
///
/// Failure is silence: a registry is a convenience, and a session that cannot
/// write one must still start.
pub struct RememberStore<'a> {
    catalog: &'a dyn StoreCatalog,
    index: &'a dyn StoreIndex,
}

impl<'a> RememberStore<'a> {
    pub fn new(catalog: &'a dyn StoreCatalog, index: &'a dyn StoreIndex) -> Self {
        Self { catalog, index }
    }

    pub fn execute(&self, path: &Path) {
        let known = self.index.remembered().unwrap_or_default();

        // Pruned here as well as on read. A path can disappear at any time —
        // a repository deleted, a temporary directory reclaimed — and an
        // index that only ever grows becomes a log of everywhere memory has
        // ever been, which is not what it is for and is not something to
        // keep about someone.
        let live: Vec<_> = known
            .iter()
            .filter(|known| self.catalog.is_store(known))
            .cloned()
            .collect();

        if live.len() == known.len() && live.iter().any(|known| known == path) {
            return;
        }

        let mut lines = live;
        if !lines.iter().any(|known| known == path) {
            lines.push(path.to_path_buf());
        }
        let _ = self.index.replace(&lines);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::lifecycle::adapters::filesystem_store_catalog::FilesystemStoreCatalog;
    use crate::lifecycle::adapters::jsonl_store_index::JsonlStoreIndex;

    fn store_at(path: &Path, format: &str, engine: &str) {
        std::fs::create_dir_all(path.join("store")).expect("store dir");
        std::fs::write(path.join("FORMAT_VERSION"), format).expect("stamp");
        std::fs::write(
            path.join(format!("store/kernel.{engine}")),
            vec![0u8; 1_024],
        )
        .expect("store file");
    }

    #[test]
    fn remembering_the_same_store_twice_writes_one_line() {
        let base = tempfile::tempdir().expect("temp");
        let data_home = base.path().join("data");
        let project = base.path().join("repo/.kernel");
        store_at(&project, "1", "bin");

        let catalog = FilesystemStoreCatalog::new(&data_home);
        let index = JsonlStoreIndex::new(&data_home);
        let remember = RememberStore::new(&catalog, &index);
        remember.execute(&project);
        remember.execute(&project);
        remember.execute(&project);
        assert_eq!(
            index.remembered().unwrap_or_default().len(),
            1,
            "an index that grows on every startup is a log, not an index"
        );
    }

    #[test]
    fn a_dead_path_is_dropped_from_the_index_the_next_time_anything_is_remembered() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let data_home = base.join("data");
        let gone = base.join("deleted/.kernel");
        let kept = base.join("kept/.kernel");
        store_at(&gone, "1", "bin");
        store_at(&kept, "2", "sqlite3");

        let catalog = FilesystemStoreCatalog::new(&data_home);
        let index = JsonlStoreIndex::new(&data_home);
        let remember = RememberStore::new(&catalog, &index);
        remember.execute(&gone);
        std::fs::remove_dir_all(base.join("deleted")).expect("the repository was deleted");
        remember.execute(&kept);

        // An index that only ever grows becomes a log of everywhere memory
        // has ever been, which is not what it is for.
        assert_eq!(
            index.remembered().unwrap_or_default(),
            vec![kept],
            "the dead path should be gone"
        );
    }
}
