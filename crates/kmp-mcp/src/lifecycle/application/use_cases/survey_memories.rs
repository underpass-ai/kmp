use std::path::PathBuf;

use crate::lifecycle::domain::memory_record::MemoryRecord;
use crate::lifecycle::domain::reach_rules::ReachRules;
use crate::lifecycle::ports::store_catalog::StoreCatalog;
use crate::lifecycle::ports::store_index::StoreIndex;

/// Use case: every memory this machine can be shown to hold.
///
/// Nothing could answer that. `info` and `doctor` reported one data
/// directory — the one *this shell* would open — and every other store was
/// invisible to every command that ships. Two halves, and they cost
/// different things: user-scope stores are found by the catalog with no new
/// state, orphans included; project stores can be anywhere, so they come
/// from the remembered index. A path that has since disappeared is dropped
/// rather than listed as live.
///
/// How each one is reached is the reach rules' decision, never a guess from
/// which half found it (#903): a remembered store can be the saved selection,
/// and a store under the data home can be the one `KMP_MCP_DATA_DIR` opens.
pub struct SurveyMemories<'a> {
    catalog: &'a dyn StoreCatalog,
    index: &'a dyn StoreIndex,
    rules: Option<ReachRules>,
}

impl<'a> SurveyMemories<'a> {
    pub fn new(catalog: &'a dyn StoreCatalog, index: &'a dyn StoreIndex) -> Self {
        Self {
            catalog,
            index,
            rules: None,
        }
    }

    /// Label each store with these rules. Without them only the per-user
    /// default and project `.kernel` directories are known to be reachable.
    pub fn with_reach(mut self, rules: ReachRules) -> Self {
        self.rules = Some(rules);
        self
    }

    pub fn execute(&self) -> Vec<MemoryRecord> {
        let rules = self
            .rules
            .clone()
            .unwrap_or_else(|| ReachRules::new(self.catalog.user_default_store()));

        let mut paths: Vec<PathBuf> = self.catalog.user_scope_stores();
        for path in self.index.remembered().unwrap_or_default() {
            if !self.catalog.is_store(&path) {
                continue; // pruned: the directory is gone
            }
            paths.push(path);
        }

        paths.sort();
        paths.dedup();
        paths
            .into_iter()
            .map(|path| {
                let facts = self.catalog.store_facts(&path);
                MemoryRecord {
                    reach: rules.reach_of(&path),
                    path,
                    storage: facts.storage,
                    size: facts.size,
                    last_opened: facts.last_opened,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use kmp_embedded::SUPPORTED_FORMAT_VERSION;

    use super::*;
    use crate::lifecycle::adapters::filesystem_store_catalog::FilesystemStoreCatalog;
    use crate::lifecycle::adapters::jsonl_store_index::JsonlStoreIndex;
    use crate::lifecycle::application::use_cases::remember_store::RememberStore;
    use crate::lifecycle::domain::store_reach::StoreReach;
    use crate::lifecycle::domain::store_storage::StoreStorage;

    fn store_at(path: &Path, format: &str, engine: &str) {
        std::fs::create_dir_all(path.join("store")).expect("store dir");
        std::fs::write(path.join("FORMAT_VERSION"), format).expect("stamp");
        std::fs::write(
            path.join(format!("store/kernel.{engine}")),
            vec![0u8; 1_024],
        )
        .expect("store file");
    }

    fn survey(data_home: &Path) -> Vec<MemoryRecord> {
        let catalog = FilesystemStoreCatalog::new(data_home);
        let index = JsonlStoreIndex::new(data_home);
        SurveyMemories::new(&catalog, &index).execute()
    }

    #[test]
    fn a_store_under_the_data_home_that_no_rule_reaches_is_listed_and_labelled() {
        let base = tempfile::tempdir().expect("temp");
        let data_home = base.path();
        let supported = SUPPORTED_FORMAT_VERSION.to_string();
        store_at(&data_home.join("kmp/default"), &supported, "sqlite3");
        store_at(&data_home.join("kmp/retired-2026-08-17"), "1", "bin");
        store_at(&data_home.join("kmp/shared"), &supported, "sqlite3");

        let memories = survey(data_home);
        assert_eq!(memories.len(), 3, "{memories:?}");

        let unreachable: Vec<_> = memories
            .iter()
            .filter(|memory| memory.reach == StoreReach::Unreachable)
            .collect();
        // The two nothing would ever mention again: a pre-migration backup and
        // whatever a retired migration workflow left behind.
        assert_eq!(unreachable.len(), 2, "{unreachable:?}");
        assert!(
            memories
                .iter()
                .any(|memory| memory.reach == StoreReach::User
                    && memory.storage == Some(StoreStorage::Sqlite))
        );
        assert!(
            memories.iter().any(|memory| memory.storage
                == Some(StoreStorage::UnsupportedFormat(Some("1".to_string())))),
            "the format-1 backup stays labelled unsupported: {memories:?}"
        );
    }

    #[test]
    fn a_project_store_is_listed_from_anywhere_once_it_has_been_remembered() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let data_home = base.join("data");
        let project = base.join("repo/.kernel");
        store_at(&project, "1", "bin");

        // Nothing knows about it yet: it is not under the data home.
        assert!(survey(&data_home).is_empty());

        let catalog = FilesystemStoreCatalog::new(&data_home);
        let index = JsonlStoreIndex::new(&data_home);
        RememberStore::new(&catalog, &index).execute(&project);

        let memories = survey(&data_home);
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].reach, StoreReach::Project);
        assert_eq!(
            memories[0].storage,
            Some(StoreStorage::UnsupportedFormat(Some("1".to_string())))
        );
    }

    #[test]
    fn a_remembered_store_is_labelled_by_the_rule_that_reaches_it_not_by_the_index() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let data_home = base.join("data");
        let supported = SUPPORTED_FORMAT_VERSION.to_string();
        let saved = base.join("game/.kernel");
        let scratch = base.join("scratch/store");
        let shared = data_home.join("kmp/shared");
        store_at(&saved, &supported, "sqlite3");
        store_at(&scratch, &supported, "sqlite3");
        store_at(&shared, &supported, "sqlite3");
        let catalog = FilesystemStoreCatalog::new(&data_home);
        let index = JsonlStoreIndex::new(&data_home);
        RememberStore::new(&catalog, &index).execute(&saved);
        RememberStore::new(&catalog, &index).execute(&scratch);

        let rules = ReachRules::new(data_home.join("kmp/default"))
            .opening(shared.clone(), StoreReach::Env)
            .saved(saved.clone());
        let memories = SurveyMemories::new(&catalog, &index)
            .with_reach(rules)
            .execute();
        let reach = |path: &Path| {
            memories
                .iter()
                .find(|memory| memory.path == path)
                .map(|memory| memory.reach)
        };
        assert_eq!(reach(&saved), Some(StoreReach::Saved));
        assert_eq!(reach(&shared), Some(StoreReach::Env));
        assert_eq!(
            reach(&scratch),
            Some(StoreReach::Unreachable),
            "a remembered store no rule resolves to is not a project"
        );
    }

    #[test]
    fn a_path_that_is_gone_is_pruned_rather_than_listed_as_live() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let data_home = base.join("data");
        let project = base.join("repo/.kernel");
        store_at(&project, "1", "bin");
        let catalog = FilesystemStoreCatalog::new(&data_home);
        let index = JsonlStoreIndex::new(&data_home);
        RememberStore::new(&catalog, &index).execute(&project);

        std::fs::remove_dir_all(&project).expect("the repository was deleted");

        let indexed = index.remembered().unwrap_or_default();
        assert_eq!(indexed.len(), 1, "the index still names it");
        assert!(
            survey(&data_home).is_empty(),
            "a registry that lists dead entries as live is its own bug"
        );
    }

    #[test]
    fn a_machine_with_no_memory_lists_nothing_rather_than_failing() {
        let base = tempfile::tempdir().expect("temp");
        assert!(survey(base.path()).is_empty());
    }
}
