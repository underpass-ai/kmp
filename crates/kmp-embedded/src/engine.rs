//! Which storage engine a fresh data directory gets
//! ([ADR-018](../../../docs/adr/ADR-018-multi-process-embedded-store.md)).
//!
//! The choice is made once, when a directory is created, and recorded in its
//! `FORMAT_VERSION`. `KMP_MCP_ENGINE` says what a *fresh* directory should
//! be; an existing directory opens with the engine it was created with, and
//! asking for a different one is refused rather than quietly ignored — a
//! store that opens as the wrong engine behind a user's back is exactly the
//! silent divergence that ends in "why can't my second host open it".

use kmp_adapter_embedded::StorageEngine;
use kmp_domain::PortError;

/// Which engine to create a fresh data directory with: `redb` (default) or
/// `sqlite`. Ignored for an existing directory only in the sense that it
/// must agree with it.
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
}
