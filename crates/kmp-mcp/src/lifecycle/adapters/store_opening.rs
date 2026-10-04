use kmp_domain::PortError;
use kmp_embedded::ResolvedDataDir;

use crate::lifecycle::adapters::filesystem_store_catalog::FilesystemStoreCatalog;
use crate::lifecycle::adapters::jsonl_store_index::JsonlStoreIndex;
use crate::lifecycle::application::use_cases::remember_store::RememberStore;

/// Resolves the store this process is about to use, prepares it, and
/// remembers it in the machine's index.
///
/// The one door every command that opens memory goes through — `serve`,
/// `export`, `import`, `document`, `snapshot`, `consolidation`, `summaries`,
/// `viewer` — so the inventory sees a store whichever of them touched it
/// first. Only `serve` remembered before (#903), and a project store only
/// ever exported or browsed stayed invisible from every other directory.
///
/// Diagnostics (`info`, `doctor`, `config`) locate instead and never come
/// here: reporting where memory would live must not change the answer.
pub fn resolve_memory_for_use() -> Result<ResolvedDataDir, PortError> {
    let resolved = kmp_embedded::resolve_data_dir_from_env()?;
    remember(&resolved);
    Ok(resolved)
}

/// Failure is silence: an index is a convenience, and a command that cannot
/// write one must still run.
fn remember(resolved: &ResolvedDataDir) {
    if let Some(data_home) = kmp_embedded::user_data_home() {
        let catalog = FilesystemStoreCatalog::new(&data_home);
        let index = JsonlStoreIndex::new(&data_home);
        RememberStore::new(&catalog, &index).execute(resolved.path());
    }
}
