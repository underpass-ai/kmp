//! Which memories exist on this machine.
//!
//! Nothing could answer that. `info` and `doctor` reported one data directory
//! — the one *this shell* would open under one of the three rules — and every
//! other store was invisible to every command that ships. On one machine that
//! was one of five, and two of the five were reachable by no rule at all:
//! a pre-migration redb backup and retired migration work directories, 1.4 MB
//! of memory that nothing would ever mention again.
//!
//! The per-store startup log cannot serve as a registry, because the record of
//! what happened to a store lives inside it: reading it requires already
//! knowing where the store is.
//!
//! Two halves, and they cost different things. **User-scope stores** all live
//! under one directory, so a `readdir` filtered on `FORMAT_VERSION` finds every
//! one of them including the orphans — no new state, no scanning. **Project
//! stores** can be anywhere, so they are remembered: each time the binary
//! resolves a data directory it writes a line to a local index, which reaches
//! directories no sane scan would visit.
//!
//! The index is machine state about someone's filesystem. It stays local, it
//! never travels in a bundle, and it prunes on read — a registry that lists a
//! path that is gone is its own bug.

use std::path::{Path, PathBuf};

/// Where the index lives, under the user data home beside the stores it names.
pub const INDEX_FILE: &str = "known-stores.jsonl";

/// How a store can be reached, which is what decides whether anyone will ever
/// find it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// The per-user default: `kmp-mcp` opens it from anywhere with nothing set.
    User,
    /// A project store, reachable from inside its own repository.
    Project,
    /// No rule resolves to it. `KMP_MCP_DATA_DIR` by hand is the only way in.
    Unreachable,
}

impl Reach {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Unreachable => "unreachable",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Memory {
    pub path: PathBuf,
    pub reach: Reach,
    pub format: String,
    pub engine: Option<String>,
    pub bytes: u64,
    /// When a session last started against it, from the store's own log.
    pub last_opened: Option<String>,
}

/// Every memory this machine can be shown to hold, newest activity last.
///
/// `data_home` is where user-scope stores live; `indexed` are the project
/// stores the binary has resolved before. A path that has since disappeared is
/// dropped rather than listed as live.
pub fn list(data_home: &Path, indexed: &[PathBuf]) -> Vec<Memory> {
    let mut paths: Vec<(PathBuf, Reach)> = Vec::new();

    let user_default = data_home.join("kmp").join("default");
    if let Ok(entries) = std::fs::read_dir(data_home.join("kmp")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.join("FORMAT_VERSION").is_file() {
                continue;
            }
            let reach = if path == user_default {
                Reach::User
            } else {
                // Under the data home but not the name any rule resolves to:
                // a backup, or a store some command left behind. Nothing will
                // mention it again unless something like this does.
                Reach::Unreachable
            };
            paths.push((path, reach));
        }
    }

    for path in indexed {
        if !path.join("FORMAT_VERSION").is_file() {
            continue; // pruned: the directory is gone
        }
        if paths.iter().any(|(known, _)| known == path) {
            continue;
        }
        paths.push((path.clone(), Reach::Project));
    }

    paths.sort_by(|left, right| left.0.cmp(&right.0));
    paths.dedup_by(|left, right| left.0 == right.0);
    paths
        .into_iter()
        .map(|(path, reach)| Memory {
            format: read_trimmed(&path.join("FORMAT_VERSION")).unwrap_or_else(|| "?".to_string()),
            engine: engine_name(&path),
            bytes: directory_size(&path),
            last_opened: last_startup(&path),
            path,
            reach,
        })
        .collect()
}

/// Remembers that this data directory exists, so it can be listed from
/// anywhere later. Appends only when the path is new to the index.
///
/// Failure is silence: a registry is a convenience, and a session that cannot
/// write one must still start.
pub fn remember(data_home: &Path, path: &Path) {
    let index = data_home.join("kmp").join(INDEX_FILE);
    let known = read_index(&index);

    // Pruned here as well as on read. A path can disappear at any time — a
    // repository deleted, a temporary directory reclaimed — and an index that
    // only ever grows becomes a log of everywhere memory has ever been, which
    // is not what it is for and is not something to keep about someone.
    let live: Vec<PathBuf> = known
        .iter()
        .filter(|known| known.join("FORMAT_VERSION").is_file())
        .cloned()
        .collect();

    if live.len() == known.len() && live.iter().any(|known| known == path) {
        return;
    }

    let mut lines = live;
    if !lines.iter().any(|known| known == path) {
        lines.push(path.to_path_buf());
    }

    let Some(parent) = index.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let body = lines
        .iter()
        .map(|path| {
            format!(
                "{{\"path\":{}}}",
                serde_json::json!(path.display().to_string())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let _ = std::fs::write(&index, format!("{body}\n"));
}

/// The paths the index names, in the order they were first seen.
pub fn read_index(index: &Path) -> Vec<PathBuf> {
    let Ok(contents) = std::fs::read_to_string(index) else {
        return Vec::new();
    };
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

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
}

fn engine_name(store: &Path) -> Option<String> {
    let store_dir = store.join("store");
    if store_dir.join("kernel.sqlite3").is_file() {
        return Some("sqlite".to_string());
    }
    if store_dir.join("kernel.redb").is_file() {
        return Some("redb".to_string());
    }
    None
}

/// The newest startup this store recorded, from its own rotating log.
fn last_startup(store: &Path) -> Option<String> {
    let mut newest: Option<String> = None;
    for entry in std::fs::read_dir(store.join("logs")).ok()?.flatten() {
        let Ok(contents) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for line in contents.lines() {
            let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let message = event["fields"]["message"].as_str().unwrap_or_default();
            if message != "startup succeeded" && message != "startup failed" {
                continue;
            }
            let Some(when) = event["timestamp"].as_str() else {
                continue;
            };
            let when = when.get(..19).unwrap_or(when).replace('T', " ");
            if newest
                .as_deref()
                .is_none_or(|current| current < when.as_str())
            {
                newest = Some(when);
            }
        }
    }
    newest
}

fn directory_size(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if metadata.is_symlink() {
        return 0;
    }
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| directory_size(&entry.path()))
        .sum()
}

/// A size a person reads at a glance.
pub fn human_size(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1}M", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{}K", bytes / 1_024)
    } else {
        format!("{bytes}B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_store_under_the_data_home_that_no_rule_reaches_is_listed_and_labelled() {
        let base = tempfile::tempdir().expect("temp");
        let data_home = base.path();
        store_at(&data_home.join("kmp/default"), "2", "sqlite3");
        store_at(&data_home.join("kmp/default-redb-2026-08-17"), "1", "redb");
        store_at(&data_home.join("kmp/shared"), "2", "sqlite3");

        let memories = list(data_home, &[]);
        assert_eq!(memories.len(), 3, "{memories:?}");

        let unreachable: Vec<_> = memories
            .iter()
            .filter(|memory| memory.reach == Reach::Unreachable)
            .collect();
        // The two nothing would ever mention again: a pre-migration backup and
        // whatever a retired migration workflow left behind.
        assert_eq!(unreachable.len(), 2, "{unreachable:?}");
        assert!(memories.iter().any(
            |memory| memory.reach == Reach::User && memory.engine.as_deref() == Some("sqlite")
        ));
    }

    #[test]
    fn a_project_store_is_listed_from_anywhere_once_it_has_been_remembered() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let data_home = base.join("data");
        let project = base.join("repo/.kernel");
        store_at(&project, "1", "redb");

        // Nothing knows about it yet: it is not under the data home.
        assert!(list(&data_home, &[]).is_empty());

        remember(&data_home, &project);
        let memories = list(
            &data_home,
            &read_index(&data_home.join("kmp").join(INDEX_FILE)),
        );
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].reach, Reach::Project);
        assert_eq!(memories[0].engine.as_deref(), Some("redb"));
    }

    #[test]
    fn remembering_the_same_store_twice_writes_one_line() {
        let base = tempfile::tempdir().expect("temp");
        let data_home = base.path().join("data");
        let project = base.path().join("repo/.kernel");
        store_at(&project, "1", "redb");

        remember(&data_home, &project);
        remember(&data_home, &project);
        remember(&data_home, &project);
        assert_eq!(
            read_index(&data_home.join("kmp").join(INDEX_FILE)).len(),
            1,
            "an index that grows on every startup is a log, not an index"
        );
    }

    #[test]
    fn a_path_that_is_gone_is_pruned_rather_than_listed_as_live() {
        let base = tempfile::tempdir().expect("temp");
        let data_home = base.path().join("data");
        let project = base.path().join("repo/.kernel");
        store_at(&project, "1", "redb");
        remember(&data_home, &project);

        std::fs::remove_dir_all(&project).expect("the repository was deleted");

        let indexed = read_index(&data_home.join("kmp").join(INDEX_FILE));
        assert_eq!(indexed.len(), 1, "the index still names it");
        assert!(
            list(&data_home, &indexed).is_empty(),
            "a registry that lists dead entries as live is its own bug"
        );
    }

    #[test]
    fn a_dead_path_is_dropped_from_the_index_the_next_time_anything_is_remembered() {
        let base = tempfile::tempdir().expect("temp");
        let base = base.path();
        let data_home = base.join("data");
        let gone = base.join("deleted/.kernel");
        let kept = base.join("kept/.kernel");
        store_at(&gone, "1", "redb");
        store_at(&kept, "2", "sqlite3");

        remember(&data_home, &gone);
        std::fs::remove_dir_all(base.join("deleted")).expect("the repository was deleted");
        remember(&data_home, &kept);

        let index = read_index(&data_home.join("kmp").join(INDEX_FILE));
        // An index that only ever grows becomes a log of everywhere memory
        // has ever been, which is not what it is for.
        assert_eq!(index, vec![kept], "{index:?}");
    }

    #[test]
    fn the_last_startup_comes_from_the_stores_own_rotating_log() {
        let base = tempfile::tempdir().expect("temp");
        let data_home = base.path();
        let store = data_home.join("kmp/default");
        store_at(&store, "2", "sqlite3");
        std::fs::create_dir_all(store.join("logs")).expect("logs");
        // The log rolls, so the newest line is not in the newest-named file
        // by construction; take the newest timestamp, not the last file.
        std::fs::write(
            store.join("logs/kmp-mcp.log.2026-08-20"),
            "{\"timestamp\":\"2026-08-20T09:00:00Z\",\"fields\":{\"message\":\"startup succeeded\"}}\n",
        )
        .expect("older log");
        std::fs::write(
            store.join("logs/kmp-mcp.log.2026-08-24"),
            "{\"timestamp\":\"2026-08-24T18:30:00Z\",\"fields\":{\"message\":\"startup succeeded\"}}\n",
        )
        .expect("newer log");

        let memories = list(data_home, &[]);
        assert_eq!(
            memories[0].last_opened.as_deref(),
            Some("2026-08-24 18:30:00")
        );
    }

    #[test]
    fn a_machine_with_no_memory_lists_nothing_rather_than_failing() {
        let base = tempfile::tempdir().expect("temp");
        assert!(list(base.path(), &[]).is_empty());
    }
}
