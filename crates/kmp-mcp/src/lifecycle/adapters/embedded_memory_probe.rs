use std::path::{Path, PathBuf};

use kmp_embedded::{ResolvedDataDir, StorageEngine};

use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;

// The embedded layout, read and never prepared: what this binary is,
// which store this shell would open, and what is actually on its disk.
/// The engines this build carries, as `--version` reports them.
pub(crate) fn compiled_formats() -> String {
    format!("{} (sqlite)", StorageEngine::Sqlite.format_version())
}

/// Whether the compiled SQLite engine's store file is on disk.
pub(crate) fn engine_on_disk(data_dir: &Path) -> Option<StorageEngine> {
    kmp_embedded::store_file_path_for(data_dir, StorageEngine::Sqlite)
        .exists()
        .then_some(StorageEngine::Sqlite)
}

pub(crate) fn store_file_on_disk(data_dir: &Path) -> Option<PathBuf> {
    let sqlite = kmp_embedded::store_file_path_for(data_dir, StorageEngine::Sqlite);
    if sqlite.exists() {
        return Some(sqlite);
    }
    let mut artifacts = std::fs::read_dir(data_dir.join("store"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    artifacts.sort();
    artifacts.into_iter().next()
}

pub(crate) fn describe_data_dir(resolved: &ResolvedDataDir) -> LifecycleFinding {
    let path = resolved.path();
    let layout = kmp_embedded::validate_store_layout(path);
    let mut finding = match &layout {
        Ok(_) => LifecycleFinding::new(DiagnosticSeverity::Ok, path.display().to_string()),
        Err(error) => LifecycleFinding::new(
            DiagnosticSeverity::Fail,
            "the selected memory cannot be opened",
        )
        .with_detail(format!("data dir: {}", path.display()))
        .with_detail(error.to_string())
        .with_detail("the diagnostic left every store file untouched"),
    }
    .with_detail(format!("chosen by: {}", resolved.rule_name()));

    match layout {
        Ok(Some(engine)) => {
            finding = finding.with_detail(format!("store format: {}", engine.format_version()));
        }
        Ok(None) => finding = finding.with_detail("store format: not stamped yet"),
        Err(_) => {}
    }
    match engine_on_disk(path) {
        Some(engine) => finding = finding.with_detail(format!("engine on disk: {engine}")),
        None => match store_file_on_disk(path) {
            Some(artifact) => {
                finding = finding
                    .with_detail("storage artifact on disk: unsupported; source left untouched")
                    .with_detail(format!("artifact: {}", artifact.display()));
            }
            None => finding = finding.with_detail("no store yet — it is created on first write"),
        },
    }
    if let Some(bundle) = kmp_embedded::project_bundle_path(resolved) {
        finding = finding.with_detail(format!(
            "committed memory: {} ({})",
            bundle.display(),
            if bundle.exists() {
                "present"
            } else {
                "not exported yet"
            }
        ));
    }
    // Whether `ask` on this store crosses languages is a fact about the
    // store's directory, and the one an operator cannot otherwise see.
    finding
        .with_detail(crate::serving::adapters::lexical_bridge_file::describe_lexical_bridge(path))
}

pub(crate) fn data_dir_finding() -> (LifecycleFinding, Option<ResolvedDataDir>) {
    // Locate, never prepare: a report on where memory lives must not create
    // it. `info` and `doctor` are run from wherever the user is standing.
    match kmp_embedded::locate_data_dir_from_env() {
        Ok(resolved) => (describe_data_dir(&resolved), Some(resolved)),
        Err(error) => (
            LifecycleFinding::new(
                DiagnosticSeverity::Fail,
                "the data directory does not resolve",
            )
            .with_detail(error.to_string())
            .with_detail("nothing can be read or written until this resolves"),
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::adapters::startup_log_probe::startup_history;
    use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
    use kmp_embedded::{ResolvedDataDir, StorageEngine};

    #[test]
    fn the_compiled_formats_name_the_engines_this_build_carries() {
        let formats = compiled_formats();
        assert!(!formats.contains("legacy read"), "{formats}");
        assert!(formats.contains("2 (sqlite)"), "{formats}");
    }
    #[test]
    fn an_empty_directory_reports_no_engine_rather_than_guessing() {
        let empty = tempfile::tempdir().expect("tempdir");
        assert!(engine_on_disk(empty.path()).is_none());
    }
    #[test]
    fn an_unopenable_layout_is_a_failure_and_never_claims_memory_is_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = kmp_embedded::store_file_path_for(dir.path(), StorageEngine::Sqlite);
        std::fs::create_dir_all(store.parent().expect("parent")).expect("store dir");
        std::fs::write(&store, b"memory is still present").expect("store marker");
        let resolved = ResolvedDataDir::Explicit(dir.path().to_path_buf());

        for stamp in [Some("3\n"), Some("banana\n"), None] {
            match stamp {
                Some(stamp) => std::fs::write(kmp_embedded::format_version_path(dir.path()), stamp)
                    .expect("write invalid stamp"),
                None => std::fs::remove_file(kmp_embedded::format_version_path(dir.path()))
                    .expect("remove stamp"),
            }
            let finding = describe_data_dir(&resolved);
            assert_eq!(finding.severity(), DiagnosticSeverity::Fail, "{finding:?}");
            assert!(
                finding.headline().contains("cannot be opened"),
                "{finding:?}"
            );
            assert!(
                finding
                    .detail()
                    .iter()
                    .any(|line| line == "engine on disk: sqlite"),
                "{finding:?}"
            );
            assert!(
                finding
                    .detail()
                    .iter()
                    .all(|line| !line.contains("no store yet")),
                "{finding:?}"
            );
            assert!(store.exists(), "diagnosis must preserve the memory file");
        }
    }
    #[test]
    fn an_unknown_storage_artifact_is_preserved_without_naming_an_engine() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = dir.path().join("store");
        std::fs::create_dir_all(&store).expect("store dir");
        std::fs::write(store.join("retired-layout.bin"), b"not a real store").expect("write");
        assert!(engine_on_disk(dir.path()).is_none());
        assert_eq!(
            store_file_on_disk(dir.path()),
            Some(store.join("retired-layout.bin"))
        );
    }
    #[test]
    fn neither_command_opens_or_creates_a_store() {
        // The promise the plugin's doctor made and this one inherits: a
        // diagnostic must not create a store as a side effect, nor take the
        // single-writer lock out from under a live session.
        let dir = tempfile::tempdir().expect("tempdir");
        let before: Vec<_> = std::fs::read_dir(dir.path()).expect("read").collect();
        assert!(before.is_empty());

        let _ = engine_on_disk(dir.path());
        let _ = startup_history(dir.path(), 5);

        let after: Vec<_> = std::fs::read_dir(dir.path()).expect("read").collect();
        assert!(after.is_empty(), "the directory is untouched");
    }
}
