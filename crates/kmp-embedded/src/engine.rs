//! Which storage engine a fresh data directory gets.
//!
//! The choice is made once, when a directory is created, and recorded in its
//! `FORMAT_VERSION`. SQLite is the only engine, so a fresh directory gets it
//! and an existing supported directory opens from its stamp. Retired layouts
//! are rejected rather than reinterpreted. `KMP_MCP_ENGINE`, which could only
//! say `sqlite`, was removed at 1.0.

use std::path::Path;

use kmp_adapter_embedded::StorageEngine;

/// Implicit engine choice for a data directory when no operator override is
/// present. Existing stores defer to their stamp; fresh stores use SQLite.
pub fn default_engine_for_data_dir(data_dir: &Path) -> Option<StorageEngine> {
    if data_dir.join("FORMAT_VERSION").exists() {
        return None;
    }
    Some(StorageEngine::Sqlite)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_sqlite_is_only_the_implicit_choice_for_a_fresh_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let fresh = temp.path().join("fresh");
        assert_eq!(
            default_engine_for_data_dir(&fresh),
            Some(StorageEngine::Sqlite)
        );

        std::fs::create_dir_all(&fresh).expect("data dir");
        std::fs::write(fresh.join("FORMAT_VERSION"), "1\n").expect("stamp");
        assert_eq!(
            default_engine_for_data_dir(&fresh),
            None,
            "an existing store must be opened from its stamp"
        );
    }
}
