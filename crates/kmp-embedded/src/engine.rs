//! Which storage engine a fresh data directory gets
//! ([ADR-018](../../../archive/docs/adr/ADR-018-multi-process-embedded-store.md)).
//!
//! The choice is made once, when a directory is created, and recorded in its
//! `FORMAT_VERSION`. `KMP_MCP_ENGINE` says what a *fresh* directory should
//! be; an existing directory opens with the engine it was created with, and
//! asking for a different one is refused rather than quietly ignored — a
//! store that opens as the wrong engine behind a user's back is exactly the
//! silent divergence that ends in "why can't my second host open it".

use std::path::Path;

use kmp_adapter_embedded::StorageEngine;
use kmp_domain::PortError;

/// Explicit engine override for a fresh data directory. Without it, the
/// user-facing binary chooses SQLite when compiled and redb otherwise;
/// existing directories always open from their stamp.
pub const ENGINE_ENV: &str = "KMP_MCP_ENGINE";

/// Parses an engine name the way the environment variable and the CLI spell
/// it. Pure, for testing; [`resolve_engine_from_env`] feeds it.
pub fn parse_engine(value: &str) -> Result<StorageEngine, PortError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "redb" => Ok(StorageEngine::Redb),
        "sqlite" => Ok(StorageEngine::Sqlite),
        other => Err(PortError::InvalidState(format!(
            "unknown storage engine `{other}`; {ENGINE_ENV} accepts `redb` or `sqlite`"
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
/// an existing store is opened as stamped; a fresh store prefers SQLite when
/// this build ships it, otherwise it retains the pure-Rust redb fallback.
/// This is intentionally data-dir-aware: returning SQLite blindly would make
/// an upgraded binary refuse every existing redb store.
pub fn resolve_engine_for_data_dir_from_env(
    data_dir: &Path,
) -> Result<Option<StorageEngine>, PortError> {
    if let Some(engine) = resolve_engine_from_env()? {
        return Ok(Some(engine));
    }
    Ok(default_engine_for_data_dir(data_dir))
}

/// Implicit engine choice for a data directory when no operator override is
/// present. Existing stores defer to their stamp; fresh stores prefer the
/// shareable engine when this build carries it.
pub fn default_engine_for_data_dir(data_dir: &Path) -> Option<StorageEngine> {
    if data_dir.join("FORMAT_VERSION").exists() {
        return None;
    }
    StorageEngine::Sqlite
        .is_compiled()
        .then_some(StorageEngine::Sqlite)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_names_are_case_insensitive_and_trimmed() {
        assert_eq!(
            parse_engine("redb").expect("redb parses"),
            StorageEngine::Redb
        );
        assert_eq!(
            parse_engine(" SQLite ").expect("sqlite parses"),
            StorageEngine::Sqlite
        );
    }

    #[test]
    fn unknown_engine_names_the_variable_and_the_choices() {
        let error = parse_engine("postgres").expect_err("not an embedded engine");
        let message = error.to_string();
        assert!(message.contains("postgres"), "{message}");
        assert!(message.contains(ENGINE_ENV), "{message}");
        assert!(message.contains("`redb` or `sqlite`"), "{message}");
    }

    #[test]
    fn compiled_sqlite_is_only_the_implicit_choice_for_a_fresh_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let fresh = temp.path().join("fresh");
        let expected = StorageEngine::Sqlite
            .is_compiled()
            .then_some(StorageEngine::Sqlite);
        assert_eq!(default_engine_for_data_dir(&fresh), expected);

        std::fs::create_dir_all(&fresh).expect("data dir");
        std::fs::write(fresh.join("FORMAT_VERSION"), "1\n").expect("stamp");
        assert_eq!(
            default_engine_for_data_dir(&fresh),
            None,
            "an existing store must be opened from its stamp"
        );
    }
}
