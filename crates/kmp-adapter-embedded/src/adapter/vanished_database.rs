//! A store whose database file is gone, beside files that prove it held
//! memory.
//!
//! Opening such a directory used to create a new, empty database and serve
//! it as if nothing had happened: reads answered "not found", the next write
//! became event 1, and nothing said the history was lost. The stamp alone
//! cannot tell this apart from a store that was never written — stamping
//! without opening is a normal path — so the test is evidence that events
//! existed, not the stamp.

use std::path::{Path, PathBuf};

use kmp_domain::PortError;

/// Beside the store, the lexical index KMP keeps from the log.
const LEXICAL_INDEX_FILE: &str = "lexical-index.sqlite3";

/// The same refusal for a diagnosis that does not open the store: the
/// SQLite store file this directory would open.
pub fn refuse_vanished_store_database(data_dir: &Path) -> Result<(), PortError> {
    let store_file = super::format_version::store_file_path_for(
        data_dir,
        super::format_version::StorageEngine::Sqlite,
    );
    refuse_vanished_database(data_dir, &store_file)
}

/// Refuses to let `store_file` be created empty when the directory shows it
/// held memory before. Silent for a fresh directory and for a database that
/// is present and non-empty.
pub(super) fn refuse_vanished_database(
    data_dir: &Path,
    store_file: &Path,
) -> Result<(), PortError> {
    let state = match std::fs::metadata(store_file) {
        Ok(metadata) if metadata.len() > 0 => return Ok(()),
        Ok(_) => "empty",
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "missing",
        Err(_) => return Ok(()),
    };
    let evidence = evidence_of_earlier_memory(data_dir, store_file);
    if evidence.is_empty() {
        return Ok(());
    }
    Err(PortError::InvalidState(format!(
        "embedded store at `{dir}` lost its database: `{file}` is {state}, but this directory \
         shows it held memory before ({evidence}). Refusing to start an empty store over it, \
         which would read as if nothing had ever been remembered. Restore `{file}` from a backup \
         and reopen. For a project store, move `{dir}` aside instead: the next session rebuilds \
         it from the committed `.kmp/memory.jsonl`. To start this directory over empty on \
         purpose, move it aside as well.",
        dir = data_dir.display(),
        file = store_file.display(),
        evidence = evidence
            .iter()
            .map(|path| format!("`{}`", path.display()))
            .collect::<Vec<_>>()
            .join(", "),
    )))
}

/// Files that only exist once memory did: the proof the store writes on its
/// first append (however the events arrived: a session, `demo`, `import`, a
/// project rebuild), the telemetry salt (written by the first call that
/// answered from the store), a write-ahead log with frames still in it, and
/// a lexical index holding indexed memory.
fn evidence_of_earlier_memory(data_dir: &Path, store_file: &Path) -> Vec<PathBuf> {
    let mut evidence = Vec::new();
    let proof = super::memory_proof::memory_proof_path(data_dir);
    if proof.is_file() {
        evidence.push(proof);
    }
    let salt = data_dir.join(kmp_observability::TELEMETRY_SALT_FILE);
    if salt.is_file() {
        evidence.push(salt);
    }
    let mut wal = store_file.as_os_str().to_owned();
    wal.push("-wal");
    let wal = PathBuf::from(wal);
    if std::fs::metadata(&wal).is_ok_and(|metadata| metadata.len() > 0) {
        evidence.push(wal);
    }
    let lexical = data_dir.join(LEXICAL_INDEX_FILE);
    if lexical_index_holds_memory(&lexical) {
        evidence.push(lexical);
    }
    evidence
}

fn lexical_index_holds_memory(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(connection) = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return false;
    };
    connection
        .query_row("SELECT 1 FROM lex_node LIMIT 1", [], |_| Ok(()))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_directory_and_a_present_database_open() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("store/kernel.sqlite3");
        assert!(refuse_vanished_database(dir.path(), &file).is_ok());

        std::fs::create_dir_all(file.parent().expect("parent")).expect("store dir");
        std::fs::write(&file, b"SQLite format 3\0").expect("db");
        std::fs::write(dir.path().join("telemetry-salt"), b"salt").expect("salt");
        assert!(refuse_vanished_database(dir.path(), &file).is_ok());
    }

    #[test]
    fn a_missing_or_empty_database_beside_evidence_is_refused_with_a_way_out() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("store/kernel.sqlite3");
        std::fs::write(dir.path().join("telemetry-salt"), b"salt").expect("salt");

        let missing = refuse_vanished_database(dir.path(), &file)
            .expect_err("missing database refused")
            .to_string();
        assert!(missing.contains("is missing"), "{missing}");
        assert!(missing.contains("telemetry-salt"), "{missing}");
        assert!(missing.contains("move"), "{missing}");

        std::fs::create_dir_all(file.parent().expect("parent")).expect("store dir");
        std::fs::write(&file, b"").expect("empty db");
        let empty = refuse_vanished_database(dir.path(), &file)
            .expect_err("empty database refused")
            .to_string();
        assert!(empty.contains("is empty"), "{empty}");
    }

    #[test]
    fn a_leftover_write_ahead_log_is_evidence() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("store/kernel.sqlite3");
        std::fs::create_dir_all(file.parent().expect("parent")).expect("store dir");
        std::fs::write(dir.path().join("store/kernel.sqlite3-wal"), b"frames").expect("wal");

        assert!(refuse_vanished_database(dir.path(), &file).is_err());
    }

    #[test]
    fn the_proof_a_store_leaves_is_evidence_without_a_salt() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("store/kernel.sqlite3");
        std::fs::write(dir.path().join("held-memory"), b"held").expect("proof");

        let missing = refuse_vanished_database(dir.path(), &file)
            .expect_err("a store with a proof and no database is refused")
            .to_string();
        assert!(missing.contains("held-memory"), "{missing}");
    }
}
