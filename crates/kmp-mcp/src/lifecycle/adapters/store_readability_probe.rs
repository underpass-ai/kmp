use kmp_embedded::{ResolvedDataDir, StorageEngine};

use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;

/// Whether the selected store's database can actually be read (S6).
///
/// A valid `FORMAT_VERSION` says what the store should be, not what its
/// database holds: with the file replaced by random bytes or cut in half,
/// the stamp still validates while every tool call fails. So the database is
/// opened read-only — nothing created, stamped or adopted — which checks its
/// schema and runs SQLite's `quick_check` over every page. A store that was
/// never written has no database yet and is not judged here; a layout that
/// does not validate is already a failure of the Memory section.
pub(crate) fn store_readability_finding(resolved: &ResolvedDataDir) -> Option<LifecycleFinding> {
    let path = resolved.path();
    if !matches!(kmp_embedded::validate_store_layout(path), Ok(Some(_))) {
        return None;
    }
    let database = kmp_embedded::store_file_path_for(path, StorageEngine::Sqlite);
    if !database.exists() {
        return None;
    }
    Some(
        match kmp_embedded::EmbeddedKernelStore::open_read_only(path) {
            Ok(_) => LifecycleFinding::new(
                DiagnosticSeverity::Ok,
                "the database opens and passes SQLite's quick_check",
            )
            .with_detail(format!("{} (opened read-only)", database.display())),
            Err(error) => LifecycleFinding::new(
                DiagnosticSeverity::Fail,
                "the selected memory's database cannot be read",
            )
            .with_detail(error.to_string())
            .with_detail("every memory tool call against this store fails until it is restored")
            .with_detail("the diagnostic opened it read-only and changed nothing"),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(path: &std::path::Path) -> ResolvedDataDir {
        ResolvedDataDir::Explicit(path.to_path_buf())
    }

    #[test]
    fn random_bytes_where_the_database_should_be_fail_the_probe() {
        let store = tempfile::tempdir().expect("store");
        std::fs::write(store.path().join("FORMAT_VERSION"), "4\n").expect("stamp");
        std::fs::create_dir_all(store.path().join("store")).expect("store dir");
        std::fs::write(
            store.path().join("store/kernel.sqlite3"),
            vec![0x5a_u8; 8_192],
        )
        .expect("garbage");

        let finding = store_readability_finding(&resolved(store.path())).expect("judged");

        assert_eq!(finding.severity(), DiagnosticSeverity::Fail, "{finding:?}");
    }

    #[test]
    fn a_store_never_written_is_not_judged_and_is_left_without_a_database() {
        let store = tempfile::tempdir().expect("store");
        std::fs::write(store.path().join("FORMAT_VERSION"), "4\n").expect("stamp");

        assert!(store_readability_finding(&resolved(store.path())).is_none());
        assert!(!store.path().join("store/kernel.sqlite3").exists());
    }
}
