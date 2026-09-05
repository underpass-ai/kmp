//! Where this installation's lexical-bridge table comes from.
//!
//! The mapping crate reads bytes and is pure about it; opening a file is the
//! adapter's job. Three places are consulted, nearest first:
//!
//! 1. `KMP_LEXICAL_BRIDGE`, when an operator names a file — this is how the
//!    judged retrieval baseline points every case at the fixture table;
//! 2. `<store>/lexical-bridge.kmpb`, a table belonging to one store;
//! 3. `<user data home>/kmp/lexical-bridge.kmpb`, the machine's table.
//!
//! The third exists because a store is selected per working directory: a
//! project `.kernel/` wins over the user default, so a per-store table would
//! have to be copied into every project that ever opens memory. The shipped
//! table is several megabytes and identical everywhere, so `setup` installs
//! one per machine and a store overrides it only when it means to.

use std::path::{Path, PathBuf};

use kmp_proto_mapping::v1beta1::LexicalBridge;

pub(crate) const LEXICAL_BRIDGE_ENV: &str = "KMP_LEXICAL_BRIDGE";
pub(crate) const LEXICAL_BRIDGE_FILE: &str = "lexical-bridge.kmpb";

/// Where a table was found, so a message can say which one it means.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BridgeOrigin {
    /// Named by an operator through `KMP_LEXICAL_BRIDGE`.
    Named,
    /// Installed beside one store.
    Store,
    /// Installed once for the machine, shared by every store.
    Machine,
}

impl BridgeOrigin {
    /// The words that follow a path, or nothing when the path speaks itself.
    fn qualifier(self) -> &'static str {
        match self {
            Self::Named => " via KMP_LEXICAL_BRIDGE",
            Self::Store => "",
            Self::Machine => ", installed for this machine",
        }
    }
}

/// The table for a store, or none.
///
/// Absent by default is the honest default: a store with no table bridges
/// nothing and `ask` behaves exactly as it did. A table that is present and
/// malformed is reported and ignored rather than allowed to refuse the
/// whole store — the table is an aid to retrieval, not a condition of it.
pub(crate) fn load_lexical_bridge(data_dir: &Path) -> LexicalBridge {
    let (path, origin) = lexical_bridge_source(data_dir);
    load_bridge_at(&path, origin)
}

/// Read one table, having already decided which one.
fn load_bridge_at(path: &Path, origin: BridgeOrigin) -> LexicalBridge {
    match std::fs::read(path) {
        Ok(bytes) => match LexicalBridge::from_bytes(&bytes) {
            Ok(bridge) => bridge,
            Err(reason) => {
                tracing::warn!(path = %path.display(), reason, "lexical bridge table ignored");
                LexicalBridge::none()
            }
        },
        Err(error) if origin == BridgeOrigin::Named => {
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
    let (path, origin) = lexical_bridge_source(data_dir);
    describe_bridge_at(&path, origin)
}

/// Say what one table is, having already decided which one.
fn describe_bridge_at(path: &Path, origin: BridgeOrigin) -> String {
    let named = origin.qualifier();
    match std::fs::read(path) {
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
        Err(_) if origin == BridgeOrigin::Named => format!(
            "lexical bridge: none; {} named by {LEXICAL_BRIDGE_ENV} could not be read",
            path.display()
        ),
        Err(_) => format!(
            "lexical bridge: none; ask matches within one language until a table is \
             installed — run `kmp-mcp setup` or place one at {}",
            path.display()
        ),
    }
}

/// The nearest table this store would read, and where it came from.
fn lexical_bridge_source(data_dir: &Path) -> (PathBuf, BridgeOrigin) {
    resolve_lexical_bridge(
        std::env::var_os(LEXICAL_BRIDGE_ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
        data_dir,
        machine_lexical_bridge_path().as_deref(),
        |candidate| candidate.is_file(),
    )
}

/// The rule itself, with the environment and the filesystem handed in.
///
/// The nearest table that exists wins outright: a store that carries its own
/// table means to, and a malformed one there is a mistake to report rather
/// than a reason to quietly reach past it for the machine's copy. When
/// nothing exists, the store path is the answer, because that is where a
/// table for this store alone would go.
fn resolve_lexical_bridge(
    named: Option<PathBuf>,
    data_dir: &Path,
    machine: Option<&Path>,
    exists: impl Fn(&Path) -> bool,
) -> (PathBuf, BridgeOrigin) {
    if let Some(named) = named {
        return (named, BridgeOrigin::Named);
    }
    let beside_store = data_dir.join(LEXICAL_BRIDGE_FILE);
    if exists(&beside_store) {
        return (beside_store, BridgeOrigin::Store);
    }
    match machine {
        Some(shared) if exists(shared) => (shared.to_path_buf(), BridgeOrigin::Machine),
        _ => (beside_store, BridgeOrigin::Store),
    }
}

/// The one table `setup` installs for every store on this machine.
pub(crate) fn machine_lexical_bridge_path() -> Option<PathBuf> {
    kmp_embedded::user_data_home().map(|home| home.join("kmp").join(LEXICAL_BRIDGE_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_without_a_table_bridges_nothing() {
        let data_dir = tempfile::tempdir().expect("the fixture is valid");

        let bridge = load_bridge_at(
            &data_dir.path().join(LEXICAL_BRIDGE_FILE),
            BridgeOrigin::Store,
        );

        assert!(bridge.is_silent());
    }

    #[test]
    fn a_malformed_table_is_ignored_rather_than_fatal() {
        let data_dir = tempfile::tempdir().expect("the fixture is valid");
        std::fs::write(data_dir.path().join(LEXICAL_BRIDGE_FILE), b"not a table")
            .expect("the fixture is valid");

        let bridge = load_bridge_at(
            &data_dir.path().join(LEXICAL_BRIDGE_FILE),
            BridgeOrigin::Store,
        );

        assert!(bridge.is_silent());
    }

    #[test]
    fn the_judged_fixture_table_loads_from_beside_the_store() {
        let data_dir = tempfile::tempdir().expect("the fixture is valid");
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../kmp-testkit/judged/lexical-bridge.kmpb");
        std::fs::copy(&fixture, data_dir.path().join(LEXICAL_BRIDGE_FILE))
            .expect("the fixture is valid");

        let bridge = load_bridge_at(
            &data_dir.path().join(LEXICAL_BRIDGE_FILE),
            BridgeOrigin::Store,
        );

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

        let table = data_dir.path().join(LEXICAL_BRIDGE_FILE);
        let absent = describe_bridge_at(&table, BridgeOrigin::Store);
        assert!(absent.starts_with("lexical bridge: none"), "{absent}");
        assert!(absent.contains("lexical-bridge.kmpb"));

        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../kmp-testkit/judged/lexical-bridge.kmpb");
        std::fs::copy(&fixture, data_dir.path().join(LEXICAL_BRIDGE_FILE))
            .expect("the fixture is valid");
        let present = describe_bridge_at(&table, BridgeOrigin::Store);
        assert!(present.contains(" words, "), "{present}");
        assert!(
            present.contains("static-similarity-mrl-multilingual"),
            "{present}"
        );

        std::fs::write(data_dir.path().join(LEXICAL_BRIDGE_FILE), b"garbage")
            .expect("the fixture is valid");
        let broken = describe_bridge_at(&table, BridgeOrigin::Store);
        assert!(broken.starts_with("lexical bridge: ignored"), "{broken}");
    }

    #[test]
    fn a_store_with_no_table_anywhere_names_its_own_path() {
        let (path, origin) = resolve_lexical_bridge(
            None,
            Path::new("/stores/one"),
            Some(Path::new("/home/data/kmp/lexical-bridge.kmpb")),
            |_| false,
        );

        assert_eq!(path, Path::new("/stores/one/lexical-bridge.kmpb"));
        assert_eq!(origin, BridgeOrigin::Store);
    }

    #[test]
    fn a_table_beside_the_store_wins_over_the_machines_copy() {
        let machine = Path::new("/home/data/kmp/lexical-bridge.kmpb");

        let (path, origin) =
            resolve_lexical_bridge(None, Path::new("/stores/one"), Some(machine), |_| true);

        assert_eq!(path, Path::new("/stores/one/lexical-bridge.kmpb"));
        assert_eq!(origin, BridgeOrigin::Store);
    }

    /// The reason the machine path exists: a project store is selected per
    /// working directory, and the shipped table is too large to copy into
    /// every one of them.
    #[test]
    fn a_store_with_no_table_of_its_own_reads_the_machines() {
        let machine = Path::new("/home/data/kmp/lexical-bridge.kmpb");

        let (path, origin) =
            resolve_lexical_bridge(None, Path::new("/stores/one"), Some(machine), |candidate| {
                candidate == machine
            });

        assert_eq!(path, machine);
        assert_eq!(origin, BridgeOrigin::Machine);
    }

    #[test]
    fn a_named_table_is_used_even_when_it_is_not_there_to_be_read() {
        let (path, origin) = resolve_lexical_bridge(
            Some(PathBuf::from("/named/table.kmpb")),
            Path::new("/stores/one"),
            Some(Path::new("/home/data/kmp/lexical-bridge.kmpb")),
            |_| true,
        );

        assert_eq!(path, Path::new("/named/table.kmpb"));
        assert_eq!(origin, BridgeOrigin::Named);
    }
}
