use std::path::Path;

use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::ports::store_catalog::StoreCatalog;
use crate::lifecycle::ports::store_index::StoreIndex;

/// Use case: remove exactly one retired store from the machine-local index.
///
/// Other live entries survive byte-for-path identity. Missing stores are
/// pruned at the same time, matching remembering; the index itself
/// disappears when there is nothing left to remember.
pub struct ForgetStore<'a> {
    catalog: &'a dyn StoreCatalog,
    index: &'a dyn StoreIndex,
}

impl<'a> ForgetStore<'a> {
    pub fn new(catalog: &'a dyn StoreCatalog, index: &'a dyn StoreIndex) -> Self {
        Self { catalog, index }
    }

    pub fn execute(&self, path: &Path) -> Result<(), LifecycleError> {
        let Some(known) = self.index.remembered() else {
            return Ok(());
        };
        let retained: Vec<_> = known
            .into_iter()
            .filter(|known| known != path && self.catalog.is_store(known))
            .collect();
        if retained.is_empty() {
            return self.index.erase();
        }
        self.index.replace(&retained)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::lifecycle::adapters::filesystem_store_catalog::FilesystemStoreCatalog;
    use crate::lifecycle::adapters::jsonl_store_index::JsonlStoreIndex;
    use crate::lifecycle::application::use_cases::remember_store::RememberStore;

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
    fn forgetting_one_store_preserves_every_other_live_index_entry() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let data_home = base.join("data");
        let retired = base.join("retired/.kernel");
        let current = base.join("current/.kernel");
        store_at(&retired, "2", "sqlite3");
        store_at(&current, "2", "sqlite3");

        let catalog = FilesystemStoreCatalog::new(&data_home);
        let index = JsonlStoreIndex::new(&data_home);
        let remember = RememberStore::new(&catalog, &index);
        remember.execute(&retired);
        remember.execute(&current);

        ForgetStore::new(&catalog, &index)
            .execute(&retired)
            .expect("retire exactly one store");

        assert_eq!(index.remembered().unwrap_or_default(), vec![current]);
        assert!(
            retired.exists(),
            "forget updates the index; uninstall removes bytes"
        );
    }
}
