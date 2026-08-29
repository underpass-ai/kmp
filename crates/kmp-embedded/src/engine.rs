//! Which storage engine a fresh data directory gets
//! ([historical ADR-018](https://github.com/underpass-ai/kmp/blob/v0.5.0/archive/docs/adr/ADR-018-multi-process-embedded-store.md)).
//!
//! The choice is made once, when a directory is created, and recorded in its
//! `FORMAT_VERSION`. `KMP_MCP_ENGINE` says what a *fresh* directory should
//! be; an existing supported directory opens from its stamp. Retired layouts
//! are rejected rather than reinterpreted.

use std::path::Path;

use kmp_adapter_embedded::StorageEngine;
use kmp_domain::PortError;

/// Compatibility environment variable. SQLite is the only accepted value.
pub const ENGINE_ENV: &str = "KMP_MCP_ENGINE";

/// Parses an engine name the way the environment variable and the CLI spell
/// it. Pure, for testing; [`resolve_engine_from_env`] feeds it.
pub fn parse_engine(value: &str) -> Result<StorageEngine, PortError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "sqlite" => Ok(StorageEngine::Sqlite),
        "redb" => Err(PortError::InvalidState(format!(
            "the redb engine is retired; unset {ENGINE_ENV} or use `sqlite`"
        ))),
        other => Err(PortError::InvalidState(format!(
            "unknown storage engine `{other}`; {ENGINE_ENV} only accepts `sqlite`"
        ))),
    }
}

/// The engine `KMP_MCP_ENGINE` asks for, if it is set. Unset or empty means
/// "no preference": a fresh directory gets the default, an existing one
/// opens as it is.
pub fn resolve_engine_from_env() -> Result<Option<StorageEngine>, PortError> {
    match std::env::var(ENGINE_ENV) {
        Ok(value) if !value.trim().is_empty() => parse_engine(&value).map(Some),
        _ => Ok(None),
    }
}

/// Resolves the engine for a particular data directory.
///
/// An explicit environment choice is always authoritative. With no choice,
/// an existing store is opened as stamped; a fresh store uses SQLite.
pub fn resolve_engine_for_data_dir_from_env(
    data_dir: &Path,
) -> Result<Option<StorageEngine>, PortError> {
    if let Some(engine) = resolve_engine_from_env()? {
        return Ok(Some(engine));
    }
    Ok(default_engine_for_data_dir(data_dir))
}

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
    fn sqlite_name_is_case_insensitive_and_trimmed() {
        assert_eq!(
            parse_engine(" SQLite ").expect("sqlite parses"),
            StorageEngine::Sqlite
        );
    }

    #[test]
    fn redb_can_no_longer_be_selected_for_a_new_store() {
        let error = parse_engine("redb").expect_err("redb is legacy-only");
        assert!(error.to_string().contains("redb engine is retired"));
    }

    #[test]
    fn unknown_engine_names_the_variable_and_the_choices() {
        let error = parse_engine("postgres").expect_err("not an embedded engine");
        let message = error.to_string();
        assert!(message.contains("postgres"), "{message}");
        assert!(message.contains(ENGINE_ENV), "{message}");
        assert!(message.contains("only accepts `sqlite`"), "{message}");
    }

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
