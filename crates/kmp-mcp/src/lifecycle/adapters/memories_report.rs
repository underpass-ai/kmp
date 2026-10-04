use std::fmt::Write as _;

use crate::lifecycle::adapters::filesystem_store_catalog::FilesystemStoreCatalog;
use crate::lifecycle::adapters::jsonl_store_index::JsonlStoreIndex;
use crate::lifecycle::adapters::machine_inventory_probe::machine_inventory;
use crate::lifecycle::application::mappers::memory_inventory_mapper::MemoryInventoryMapper;
use crate::lifecycle::application::use_cases::register_store::RegisterStore;
use crate::lifecycle::domain::memory_inventory_entry::MemoryInventoryEntry;
use crate::lifecycle::domain::store_contents::StoreContents;
use crate::lifecycle::domain::store_reach::StoreReach;

const NO_DATA_HOME: &str = "none of XDG_DATA_HOME, HOME, LOCALAPPDATA, APPDATA, or USERPROFILE \
                            is set, so there is nowhere to look";

/// `kmp-mcp memories`: every known store, how it is reached, and every about
/// inside it with its event count.
pub fn memories_report() -> Result<String, String> {
    let entries = machine_inventory().ok_or_else(|| NO_DATA_HOME.to_string())?;
    Ok(render(&entries))
}

/// `kmp-mcp memories --json`: the same inventory for machines.
pub fn memories_json() -> Result<String, String> {
    let entries = machine_inventory().ok_or_else(|| NO_DATA_HOME.to_string())?;
    serde_json::to_string_pretty(&MemoryInventoryMapper::to_dto(&entries))
        .map(|json| format!("{json}\n"))
        .map_err(|error| format!("could not encode the inventory: {error}"))
}

/// `kmp-mcp memories register <absolute-path>`: remember an existing store.
pub fn register_memory(raw: &str) -> Result<String, String> {
    let data_home = kmp_embedded::user_data_home().ok_or_else(|| NO_DATA_HOME.to_string())?;
    let catalog = FilesystemStoreCatalog::new(&data_home);
    let index = JsonlStoreIndex::new(&data_home);
    let path = RegisterStore::new(&catalog, &index)
        .execute(raw)
        .map_err(|refusal| refusal.to_string())?;
    Ok(format!(
        "registered {}\n`kmp-mcp memories` and `kmp-mcp info` now list it from any directory.\n",
        path.display()
    ))
}

fn render(entries: &[MemoryInventoryEntry]) -> String {
    let mut out = String::new();
    if entries.is_empty() {
        out.push_str(
            "No memory on this machine yet. The first write creates one; where depends on \
             where you are standing.\n",
        );
        return out;
    }
    let _ = writeln!(
        out,
        "{} {} on this machine (→ is the one opened from here)",
        entries.len(),
        if entries.len() == 1 {
            "memory"
        } else {
            "memories"
        }
    );
    for entry in entries {
        let record = &entry.record;
        let _ = writeln!(
            out,
            "\n{} {}",
            if entry.opened_here { "→" } else { " " },
            record.path.display()
        );
        let _ = writeln!(out, "    reach       {}", record.reach.as_str());
        let _ = writeln!(
            out,
            "    storage     {} · {}",
            record
                .storage
                .as_ref()
                .map(|storage| storage.label())
                .unwrap_or_else(|| "empty (never written)".to_string()),
            record.size.human()
        );
        if let Some(when) = &record.last_opened {
            let _ = writeln!(out, "    last opened {when}");
        }
        match &entry.contents {
            StoreContents::Read { abouts, last_write } => {
                if let Some(when) = last_write {
                    let _ = writeln!(out, "    last write  {when}");
                }
                let _ = writeln!(
                    out,
                    "    abouts      {} · {} events",
                    abouts.len(),
                    entry.contents.total_events()
                );
                let width = abouts
                    .iter()
                    .map(|about| about.about().chars().count())
                    .max()
                    .unwrap_or_default();
                for about in abouts {
                    let _ = writeln!(
                        out,
                        "      {:<width$}  {:>7}",
                        about.about(),
                        about.events()
                    );
                }
            }
            StoreContents::Unreadable { reason } => {
                let _ = writeln!(out, "    contents    {reason}");
            }
        }
    }
    if entries
        .iter()
        .any(|entry| entry.record.reach == StoreReach::Unreachable)
    {
        out.push_str(
            "\n`unreachable` means no rule resolves to it: open it with KMP_MCP_DATA_DIR, or \
             remove exactly it with `kmp-mcp uninstall --store <absolute path>`.\n",
        );
    }
    out.push_str(
        "\nA store no command has opened here is not listed until you name it: \
         `kmp-mcp memories register <absolute path>`.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::lifecycle::domain::about_event_count::AboutEventCount;
    use crate::lifecycle::domain::memory_record::MemoryRecord;
    use crate::lifecycle::domain::store_size::StoreSize;
    use crate::lifecycle::domain::store_storage::StoreStorage;

    fn entry(path: &str, reach: StoreReach, contents: StoreContents) -> MemoryInventoryEntry {
        MemoryInventoryEntry {
            record: MemoryRecord {
                path: PathBuf::from(path),
                reach,
                storage: Some(StoreStorage::Sqlite),
                size: StoreSize::new(2_048),
                last_opened: None,
            },
            opened_here: reach == StoreReach::Project,
            contents,
        }
    }

    #[test]
    fn the_full_list_names_every_about_and_marks_the_store_opened_here() {
        let report = render(&[
            entry(
                "/repo/.kernel",
                StoreReach::Project,
                StoreContents::read(
                    vec![
                        AboutEventCount::new("project:repo", 12),
                        AboutEventCount::new("project:repo:ops", 3),
                    ],
                    Some("2026-10-04 10:00:00".to_string()),
                ),
            ),
            entry(
                "/data/kmp/old",
                StoreReach::Unreachable,
                StoreContents::unreadable("contents not readable by this engine: busy"),
            ),
        ]);
        assert!(report.contains("→ /repo/.kernel"), "{report}");
        assert!(report.contains("abouts      2 · 15 events"), "{report}");
        assert!(report.contains("project:repo:ops"), "{report}");
        assert!(
            report.contains("last write  2026-10-04 10:00:00"),
            "{report}"
        );
        assert!(
            report.contains("contents    contents not readable"),
            "{report}"
        );
        assert!(report.contains("`unreachable` means"), "{report}");
    }
}
