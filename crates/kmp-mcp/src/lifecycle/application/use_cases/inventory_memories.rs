use crate::lifecycle::application::use_cases::survey_memories::SurveyMemories;
use crate::lifecycle::domain::memory_inventory_entry::MemoryInventoryEntry;
use crate::lifecycle::domain::reach_rules::ReachRules;
use crate::lifecycle::domain::store_contents::StoreContents;
use crate::lifecycle::domain::store_storage::StoreStorage;
use crate::lifecycle::ports::store_catalog::StoreCatalog;
use crate::lifecycle::ports::store_contents_reader::StoreContentsReader;
use crate::lifecycle::ports::store_index::StoreIndex;

/// Use case: one answer to "what memory exists on this machine, in which
/// store, and how is each reached" (#903).
///
/// The survey finds the stores and labels their reach; this adds what is
/// inside each. Only a store stamped with the supported format is read at
/// all: an older stamp is reported as what it is, and its file is never
/// opened, so nothing can migrate it by looking.
pub struct InventoryMemories<'a> {
    catalog: &'a dyn StoreCatalog,
    index: &'a dyn StoreIndex,
    reader: &'a dyn StoreContentsReader,
}

impl<'a> InventoryMemories<'a> {
    pub fn new(
        catalog: &'a dyn StoreCatalog,
        index: &'a dyn StoreIndex,
        reader: &'a dyn StoreContentsReader,
    ) -> Self {
        Self {
            catalog,
            index,
            reader,
        }
    }

    pub fn execute(&self, rules: ReachRules) -> Vec<MemoryInventoryEntry> {
        let records = SurveyMemories::new(self.catalog, self.index)
            .with_reach(rules.clone())
            .execute();
        records
            .into_iter()
            .map(|record| {
                let contents = match &record.storage {
                    Some(StoreStorage::Sqlite) | None => self.reader.contents(&record.path),
                    Some(other) => StoreContents::unreadable(format!(
                        "{}: contents not readable by this engine",
                        other.label()
                    )),
                };
                MemoryInventoryEntry {
                    opened_here: rules.is_opened_here(&record.path),
                    record,
                    contents,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use super::*;
    use crate::lifecycle::adapters::filesystem_store_catalog::FilesystemStoreCatalog;
    use crate::lifecycle::adapters::jsonl_store_index::JsonlStoreIndex;
    use crate::lifecycle::domain::about_event_count::AboutEventCount;

    /// Records which stores were opened, so a test can prove one was not.
    struct RecordingReader(Mutex<Vec<PathBuf>>);

    impl StoreContentsReader for RecordingReader {
        fn contents(&self, store: &Path) -> StoreContents {
            self.0.lock().expect("lock").push(store.to_path_buf());
            StoreContents::read(vec![AboutEventCount::new("project:a", 4)], None)
        }
    }

    fn stamped(path: &Path, format: &str) {
        std::fs::create_dir_all(path.join("store")).expect("store dir");
        std::fs::write(path.join("FORMAT_VERSION"), format).expect("stamp");
        std::fs::write(path.join("store/kernel.sqlite3"), b"old bytes").expect("file");
    }

    #[test]
    fn an_old_format_store_is_reported_unreadable_and_its_file_never_opened() {
        let base = tempfile::tempdir().expect("temp");
        let data_home = base.path();
        stamped(
            &data_home.join("kmp/default"),
            &kmp_embedded::SUPPORTED_FORMAT_VERSION.to_string(),
        );
        stamped(&data_home.join("kmp/retired"), "2");
        let catalog = FilesystemStoreCatalog::new(data_home);
        let index = JsonlStoreIndex::new(data_home);
        let reader = RecordingReader(Mutex::new(Vec::new()));

        let rules = ReachRules::new(data_home.join("kmp/default")).opening(
            data_home.join("kmp/default"),
            crate::lifecycle::StoreReach::User,
        );
        let entries = InventoryMemories::new(&catalog, &index, &reader).execute(rules);

        assert_eq!(
            *reader.0.lock().expect("lock"),
            vec![data_home.join("kmp/default")],
            "only the supported store may be opened"
        );
        let retired = entries
            .iter()
            .find(|entry| entry.record.path.ends_with("retired"))
            .expect("retired listed");
        assert_eq!(
            retired.contents,
            StoreContents::unreadable(
                "unsupported format-2 artifact: contents not readable by this engine"
            )
        );
        assert!(!retired.opened_here);
        let default = entries
            .iter()
            .find(|entry| entry.record.path.ends_with("default"))
            .expect("default listed");
        assert!(default.opened_here);
        assert_eq!(default.contents.total_events(), 4);
    }
}
