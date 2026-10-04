use std::path::Path;

use kmp_embedded::ResolvedDataDir;

use crate::lifecycle::adapters::filesystem_store_catalog::FilesystemStoreCatalog;
use crate::lifecycle::adapters::jsonl_store_index::JsonlStoreIndex;
use crate::lifecycle::adapters::sqlite_store_contents_reader::SqliteStoreContentsReader;
use crate::lifecycle::application::use_cases::inventory_memories::InventoryMemories;
use crate::lifecycle::domain::memory_inventory_entry::MemoryInventoryEntry;
use crate::lifecycle::domain::reach_rules::ReachRules;
use crate::lifecycle::domain::store_reach::StoreReach;

/// The machine's inventory as this process sees it, or `None` when there is
/// no user data home to look in. Reads only: the resolution is located, not
/// prepared, so looking never creates a store.
pub(crate) fn machine_inventory() -> Option<Vec<MemoryInventoryEntry>> {
    let data_home = kmp_embedded::user_data_home()?;
    let catalog = FilesystemStoreCatalog::new(&data_home);
    let index = JsonlStoreIndex::new(&data_home);
    Some(
        InventoryMemories::new(&catalog, &index, &SqliteStoreContentsReader)
            .execute(machine_reach_rules(&data_home)),
    )
}

/// The reach rules of this process: what the real resolver would open from
/// here and by which rule, and the saved selection.
pub(crate) fn machine_reach_rules(data_home: &Path) -> ReachRules {
    let mut rules = ReachRules::new(data_home.join("kmp").join("default"));
    if let Ok(resolved) = kmp_embedded::locate_data_dir_from_env() {
        rules = rules.opening(resolved.path().to_path_buf(), reach_of_rule(&resolved));
    }
    // A broken selection is the Store config section's to report; the
    // inventory still lists every store it can.
    if let Ok(Some(saved)) = kmp_embedded::memory_selection::saved_selection() {
        rules = rules.saved(saved.path().to_path_buf());
    }
    rules
}

/// The resolver's rule, as the inventory's reach.
pub(crate) fn reach_of_rule(resolved: &ResolvedDataDir) -> StoreReach {
    match resolved {
        ResolvedDataDir::Explicit(_) => StoreReach::Env,
        ResolvedDataDir::Saved(_) => StoreReach::Saved,
        ResolvedDataDir::Project(_) => StoreReach::Project,
        ResolvedDataDir::Worktree { .. } => StoreReach::Worktree,
        ResolvedDataDir::UserDefault(_) => StoreReach::User,
        ResolvedDataDir::UserFallback { .. } => StoreReach::UserFallback,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn every_resolver_rule_maps_to_the_reach_of_the_same_name() {
        let path = PathBuf::from("/store");
        let rules = [
            ResolvedDataDir::Explicit(path.clone()),
            ResolvedDataDir::Saved(path.clone()),
            ResolvedDataDir::Project(path.clone()),
            ResolvedDataDir::Worktree {
                path: path.clone(),
                checkout: PathBuf::from("/wt"),
            },
            ResolvedDataDir::UserDefault(path.clone()),
            ResolvedDataDir::UserFallback {
                path: path.clone(),
                orphaned_bundle: kmp_embedded::OrphanedProjectBundle {
                    bundle_path: PathBuf::from("/repo/.kmp/memory.jsonl"),
                    project_store_path: PathBuf::from("/repo/.kernel"),
                    selected_store_path: path.clone(),
                    reason: "old".to_string(),
                },
            },
        ];
        for resolved in rules {
            assert_eq!(reach_of_rule(&resolved).as_str(), resolved.rule_name());
        }
    }
}
