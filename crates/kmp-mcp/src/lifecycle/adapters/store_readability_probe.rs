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
/// does not validate, or a database lost beside proof that it held memory, is
/// already the Memory section's failure, and one cause gets one failure.
pub(crate) fn store_readability_finding(resolved: &ResolvedDataDir) -> Option<LifecycleFinding> {
    let path = resolved.path();
    if !matches!(kmp_embedded::validate_store_layout(path), Ok(Some(_)))
        || kmp_embedded::refuse_vanished_store_database(path).is_err()
    {
        return None;
    }
    let database = kmp_embedded::store_file_path_for(path, StorageEngine::Sqlite);
    let Ok(metadata) = std::fs::metadata(&database) else {
        return None;
    };
    // An empty file beside no proof of memory is what the server opens as a
    // new, empty store: say that, rather than judging a schema it never had.
    if metadata.len() == 0 {
        return Some(
            LifecycleFinding::new(
                DiagnosticSeverity::Ok,
                "the database file is empty, and nothing shows it held memory",
            )
            .with_detail(database.display().to_string())
            .with_detail("the next session initializes it as a new, empty store"),
        );
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
    fn an_empty_database_without_proof_is_what_the_server_opens_as_new() {
        let store = tempfile::tempdir().expect("store");
        std::fs::write(store.path().join("FORMAT_VERSION"), "4\n").expect("stamp");
        std::fs::create_dir_all(store.path().join("store")).expect("store dir");
        std::fs::write(store.path().join("store/kernel.sqlite3"), b"").expect("empty");

        let finding = store_readability_finding(&resolved(store.path())).expect("judged");

        assert_eq!(finding.severity(), DiagnosticSeverity::Ok, "{finding:?}");
        assert!(finding.headline().contains("empty"), "{finding:?}");
        let opened = kmp_embedded::EmbeddedKernelStore::open(store.path());
        assert!(opened.is_ok(), "the server opens it: {:?}", opened.err());
    }

    #[test]
    fn a_lost_database_is_the_memory_section_failure_alone() {
        let store = tempfile::tempdir().expect("store");
        std::fs::write(store.path().join("FORMAT_VERSION"), "4\n").expect("stamp");
        std::fs::write(store.path().join("telemetry-salt"), b"salt").expect("proof");
        std::fs::create_dir_all(store.path().join("store")).expect("store dir");
        std::fs::write(store.path().join("store/kernel.sqlite3"), b"").expect("empty");

        assert!(store_readability_finding(&resolved(store.path())).is_none());
    }

    #[test]
    fn a_store_never_written_is_not_judged_and_is_left_without_a_database() {
        let store = tempfile::tempdir().expect("store");
        std::fs::write(store.path().join("FORMAT_VERSION"), "4\n").expect("stamp");

        assert!(store_readability_finding(&resolved(store.path())).is_none());
        assert!(!store.path().join("store/kernel.sqlite3").exists());
    }
}
