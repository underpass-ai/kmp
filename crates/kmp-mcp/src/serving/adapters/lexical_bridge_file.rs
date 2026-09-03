//! Where this installation's lexical-bridge table comes from.
//!
//! The mapping crate reads bytes and is pure about it; opening a file is the
//! adapter's job. The table lives beside the store as `lexical-bridge.kmpb`,
//! or wherever `KMP_LEXICAL_BRIDGE` points — the latter is how the retrieval
//! baseline runs every judged case against the same fixture table.

use std::path::{Path, PathBuf};

use kmp_proto_mapping::v1beta1::LexicalBridge;

pub(crate) const LEXICAL_BRIDGE_ENV: &str = "KMP_LEXICAL_BRIDGE";
pub(crate) const LEXICAL_BRIDGE_FILE: &str = "lexical-bridge.kmpb";

/// The table for a store, or none.
///
/// Absent by default is the honest default: a store with no table bridges
/// nothing and `ask` behaves exactly as it did. A table that is present and
/// malformed is reported and ignored rather than allowed to refuse the
/// whole store — the table is an aid to retrieval, not a condition of it.
pub(crate) fn load_lexical_bridge(data_dir: &Path) -> LexicalBridge {
    let (path, explicit) = lexical_bridge_path(data_dir);
    match std::fs::read(&path) {
        Ok(bytes) => match LexicalBridge::from_bytes(&bytes) {
            Ok(bridge) => bridge,
            Err(reason) => {
                tracing::warn!(path = %path.display(), reason, "lexical bridge table ignored");
                LexicalBridge::none()
            }
        },
        Err(error) if explicit => {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "lexical bridge table named by {LEXICAL_BRIDGE_ENV} could not be read"
            );
            LexicalBridge::none()
        }
        Err(_) => LexicalBridge::none(),
    }
}

/// One line for `info` and `doctor`: which table this store would read, or
/// that there is none and what that means for `ask`.
pub(crate) fn describe_lexical_bridge(data_dir: &Path) -> String {
    let (path, explicit) = lexical_bridge_path(data_dir);
    let named = if explicit {
        " via KMP_LEXICAL_BRIDGE"
    } else {
        ""
    };
    match std::fs::read(&path) {
        Ok(bytes) => match LexicalBridge::from_bytes(&bytes) {
            Ok(bridge) if !bridge.is_silent() => format!(
                "lexical bridge: {} words, {} ({}{named})",
                bridge.len(),
                bridge.provenance(),
                path.display()
            ),
            Ok(_) => format!(
                "lexical bridge: an empty table at {}{named}; ask matches within one language",
                path.display()
            ),
            Err(reason) => format!(
                "lexical bridge: ignored, {reason} ({}{named}); ask matches within one language",
                path.display()
            ),
        },
        Err(_) if explicit => format!(
            "lexical bridge: none; {} named by {LEXICAL_BRIDGE_ENV} could not be read",
            path.display()
        ),
        Err(_) => format!(
            "lexical bridge: none; ask matches within one language until {} is installed",
            path.display()
        ),
    }
}

/// Where the table is looked for, and whether an operator named it.
pub(crate) fn lexical_bridge_path(data_dir: &Path) -> (PathBuf, bool) {
    match std::env::var_os(LEXICAL_BRIDGE_ENV).filter(|value| !value.is_empty()) {
        Some(named) => (PathBuf::from(named), true),
        None => (data_dir.join(LEXICAL_BRIDGE_FILE), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_without_a_table_bridges_nothing() {
        let data_dir = tempfile::tempdir().expect("the fixture is valid");

        let bridge = load_lexical_bridge(data_dir.path());

        assert!(bridge.is_silent());
    }

    #[test]
    fn a_malformed_table_is_ignored_rather_than_fatal() {
        let data_dir = tempfile::tempdir().expect("the fixture is valid");
        std::fs::write(data_dir.path().join(LEXICAL_BRIDGE_FILE), b"not a table")
            .expect("the fixture is valid");

        let bridge = load_lexical_bridge(data_dir.path());

        assert!(bridge.is_silent());
    }

    #[test]
    fn the_judged_fixture_table_loads_from_beside_the_store() {
        let data_dir = tempfile::tempdir().expect("the fixture is valid");
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../kmp-testkit/judged/lexical-bridge.kmpb");
        std::fs::copy(&fixture, data_dir.path().join(LEXICAL_BRIDGE_FILE))
            .expect("the fixture is valid");

        let bridge = load_lexical_bridge(data_dir.path());

        assert!(!bridge.is_silent());
        assert!(
            bridge
                .provenance()
                .contains("static-similarity-mrl-multilingual")
        );
        let valve = bridge
            .similarity("valvula", "valve")
            .expect("both words are in the fixture");
        assert!(valve > 0.45, "{valve}");
    }

    #[test]
    fn info_names_the_table_or_says_what_its_absence_means() {
        let data_dir = tempfile::tempdir().expect("the fixture is valid");

        let absent = describe_lexical_bridge(data_dir.path());
        assert!(absent.starts_with("lexical bridge: none"), "{absent}");
        assert!(absent.contains("lexical-bridge.kmpb"));

        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../kmp-testkit/judged/lexical-bridge.kmpb");
        std::fs::copy(&fixture, data_dir.path().join(LEXICAL_BRIDGE_FILE))
            .expect("the fixture is valid");
        let present = describe_lexical_bridge(data_dir.path());
        assert!(present.contains(" words, "), "{present}");
        assert!(
            present.contains("static-similarity-mrl-multilingual"),
            "{present}"
        );

        std::fs::write(data_dir.path().join(LEXICAL_BRIDGE_FILE), b"garbage")
            .expect("the fixture is valid");
        let broken = describe_lexical_bridge(data_dir.path());
        assert!(broken.starts_with("lexical bridge: ignored"), "{broken}");
    }

    #[test]
    fn the_default_path_sits_beside_the_store() {
        let (path, explicit) = lexical_bridge_path(Path::new("/stores/one"));

        assert_eq!(path, Path::new("/stores/one/lexical-bridge.kmpb"));
        assert!(!explicit);
    }
}
