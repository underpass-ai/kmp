use std::path::{Path, PathBuf};

use crate::lifecycle::application::use_cases::remember_store::RememberStore;
use crate::lifecycle::domain::register_refusal::RegisterRefusal;
use crate::lifecycle::ports::store_catalog::StoreCatalog;
use crate::lifecycle::ports::store_index::StoreIndex;

/// Use case: add an existing store to the machine's index by hand.
///
/// A store no verb has opened since the index existed — an old project, a
/// copy someone made — is still memory on the machine. Nothing crawls the
/// disk to find it; the operator names it. Only the index is written: the
/// store is checked, never created or touched.
pub struct RegisterStore<'a> {
    catalog: &'a dyn StoreCatalog,
    index: &'a dyn StoreIndex,
}

impl<'a> RegisterStore<'a> {
    pub fn new(catalog: &'a dyn StoreCatalog, index: &'a dyn StoreIndex) -> Self {
        Self { catalog, index }
    }

    pub fn execute(&self, raw: &str) -> Result<PathBuf, RegisterRefusal> {
        let trimmed = raw.trim();
        if trimmed.starts_with('~') {
            return Err(RegisterRefusal::UnexpandedHome(raw.to_string()));
        }
        let path = Path::new(trimmed);
        if trimmed.is_empty() || !path.is_absolute() {
            return Err(RegisterRefusal::Relative(raw.to_string()));
        }
        if !self.catalog.is_store(path) {
            return Err(RegisterRefusal::NotAStore(path.to_path_buf()));
        }
        RememberStore::new(self.catalog, self.index).execute(path);
        Ok(path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::adapters::filesystem_store_catalog::FilesystemStoreCatalog;
    use crate::lifecycle::adapters::jsonl_store_index::JsonlStoreIndex;

    struct Machine {
        root: tempfile::TempDir,
    }

    impl Machine {
        fn new() -> Self {
            Self {
                root: tempfile::tempdir().expect("temp"),
            }
        }

        fn data_home(&self) -> PathBuf {
            self.root.path().join("data")
        }

        fn register(&self, raw: &str) -> Result<PathBuf, RegisterRefusal> {
            let catalog = FilesystemStoreCatalog::new(&self.data_home());
            let index = JsonlStoreIndex::new(&self.data_home());
            RegisterStore::new(&catalog, &index).execute(raw)
        }

        fn remembered(&self) -> Option<Vec<PathBuf>> {
            JsonlStoreIndex::new(&self.data_home()).remembered()
        }
    }

    #[test]
    fn an_existing_store_is_added_to_the_index_once() {
        let machine = Machine::new();
        let store = machine.root.path().join("old-project/.kernel");
        std::fs::create_dir_all(&store).expect("store");
        std::fs::write(store.join("FORMAT_VERSION"), "2").expect("stamp");
        let raw = store.display().to_string();

        assert_eq!(machine.register(&raw), Ok(store.clone()));
        assert_eq!(machine.register(&raw), Ok(store.clone()));
        assert_eq!(machine.remembered(), Some(vec![store]));
    }

    #[test]
    fn a_relative_path_is_refused_and_nothing_is_written() {
        let machine = Machine::new();
        assert_eq!(
            machine.register("repo/.kernel"),
            Err(RegisterRefusal::Relative("repo/.kernel".to_string()))
        );
        assert_eq!(
            machine.register(""),
            Err(RegisterRefusal::Relative(String::new()))
        );
        assert_eq!(machine.remembered(), None);
    }

    #[test]
    fn an_unexpanded_home_is_refused_and_nothing_is_written() {
        let machine = Machine::new();
        assert_eq!(
            machine.register("~/repo/.kernel"),
            Err(RegisterRefusal::UnexpandedHome(
                "~/repo/.kernel".to_string()
            ))
        );
        assert_eq!(machine.remembered(), None);
    }

    #[test]
    fn a_directory_without_a_stamp_is_refused_and_left_as_it_was() {
        let machine = Machine::new();
        let plain = machine.root.path().join("documents");
        std::fs::create_dir_all(&plain).expect("plain dir");
        let missing = machine.root.path().join("nowhere/.kernel");

        for path in [&plain, &missing] {
            let refusal = machine
                .register(&path.display().to_string())
                .expect_err("not a store");
            assert_eq!(refusal, RegisterRefusal::NotAStore(path.clone()));
            assert!(refusal.to_string().contains("FORMAT_VERSION"));
        }
        assert!(!missing.exists(), "register never creates a store");
        assert_eq!(
            std::fs::read_dir(&plain).expect("plain").count(),
            0,
            "register never writes into the directory it was given"
        );
        assert_eq!(machine.remembered(), None);
    }
}
