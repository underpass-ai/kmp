use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use kmp_domain::PortError;

/// The layout this binary creates for a fresh data directory: shareable
/// SQLite ([ADR-018](../../../../archive/docs/adr/ADR-018-multi-process-embedded-store.md)).
///
/// `FORMAT_VERSION` in a data directory names the *layout* — which engine
/// wrote `store/`, and how. Bumping it is what makes a binary that predates
/// a layout refuse the directory instead of opening an empty store beside
/// it, so a new engine is a new number ([ADR-018](../../../../archive/docs/adr/ADR-018-multi-process-embedded-store.md)).
pub const SUPPORTED_FORMAT_VERSION: u32 = StorageEngine::Sqlite.format_version();

/// The logical shape of the event log — what a bundle carries and what a
/// migration translates. Independent of the engine: a redb store and a
/// SQLite store export byte-identical bundles.
pub const EVENT_FORMAT_VERSION: u32 = 1;

const FORMAT_VERSION_FILE: &str = "FORMAT_VERSION";

/// The engine behind a data directory's `store/`. Chosen once, when the
/// directory is created; recorded as its `FORMAT_VERSION`; never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageEngine {
    /// Legacy format-1 redb store. New stores never select it; the variant is
    /// retained only so older memory can be opened and migrated.
    Redb,
    /// WAL-mode SQLite: several processes may open the same store. This is
    /// the only engine used for new memory.
    Sqlite,
}

impl StorageEngine {
    /// The `FORMAT_VERSION` this engine stamps.
    pub const fn format_version(self) -> u32 {
        match self {
            StorageEngine::Redb => 1,
            StorageEngine::Sqlite => 2,
        }
    }

    /// The highest layout number any build of this crate knows about,
    /// compiled in or not. Above this the binary is simply too old.
    pub(crate) const NEWEST_KNOWN_FORMAT_VERSION: u32 = StorageEngine::Sqlite.format_version();

    pub(crate) const fn from_format_version(version: u32) -> Option<Self> {
        match version {
            1 => Some(StorageEngine::Redb),
            2 => Some(StorageEngine::Sqlite),
            _ => None,
        }
    }

    /// Both known formats remain readable during the compatibility window.
    pub const fn is_compiled(self) -> bool {
        true
    }

    pub const fn name(self) -> &'static str {
        match self {
            StorageEngine::Redb => "redb",
            StorageEngine::Sqlite => "sqlite",
        }
    }

    const fn store_file_name(self) -> &'static str {
        match self {
            StorageEngine::Redb => "kernel.redb",
            StorageEngine::Sqlite => "kernel.sqlite3",
        }
    }

    const ALL: [StorageEngine; 2] = [StorageEngine::Redb, StorageEngine::Sqlite];
}

impl std::fmt::Display for StorageEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

pub fn format_version_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FORMAT_VERSION_FILE)
}

/// The version stamped in `data_dir`, without applying the gate.
///
/// The migration needs to know what it is looking at precisely when the
/// internal `check_or_stamp` gate would refuse to open it.
pub fn read_stamped_version(data_dir: &Path) -> Result<u32, PortError> {
    let version_path = format_version_path(data_dir);
    let raw = fs::read_to_string(&version_path).map_err(|error| {
        PortError::Unavailable(format!(
            "could not read FORMAT_VERSION at `{}`: {error}",
            version_path.display()
        ))
    })?;
    raw.trim().parse().map_err(|_| {
        PortError::InvalidState(format!(
            "FORMAT_VERSION at `{}` is corrupt (`{}`)",
            version_path.display(),
            raw.trim()
        ))
    })
}

/// Where `engine` keeps its store inside `data_dir`.
pub fn store_file_path_for(data_dir: &Path, engine: StorageEngine) -> PathBuf {
    data_dir.join("store").join(engine.store_file_name())
}

/// Whether any engine's store file is present — the "half-initialized
/// layout" signal used to refuse a directory with a store but no stamp.
fn any_store_file_exists(data_dir: &Path) -> bool {
    StorageEngine::ALL
        .iter()
        .any(|engine| store_file_path_for(data_dir, *engine).exists())
}

/// The store file a stamped directory points at, if the stamp names a
/// layout this crate knows and the file is there. `None` for a fresh
/// directory. Does not apply the gate: the migration asks this about
/// directories it may be about to refuse.
pub(crate) fn existing_store_file(data_dir: &Path) -> Option<(StorageEngine, PathBuf)> {
    let version = read_stamped_version(data_dir).ok()?;
    let engine = StorageEngine::from_format_version(version)?;
    let path = store_file_path_for(data_dir, engine);
    path.exists().then_some((engine, path))
}

/// Fail-fast format check per ADR-012: stamp fresh directories with the
/// default layout, reject version mismatches and half-initialized layouts
/// explicitly — never open a store that could silently read as empty memory.
///
/// Returns the engine the directory is (now) stamped for.
pub(crate) fn check_or_stamp(data_dir: &Path) -> Result<StorageEngine, PortError> {
    check_or_stamp_as(data_dir, None)
}

/// [`check_or_stamp`] with a say in the outcome: a fresh directory is
/// stamped for `wanted`, and an existing one must already be `wanted` — a
/// store is never reinterpreted as another engine's.
pub(crate) fn check_or_stamp_as(
    data_dir: &Path,
    wanted: Option<StorageEngine>,
) -> Result<StorageEngine, PortError> {
    let version_path = format_version_path(data_dir);
    match fs::read_to_string(&version_path) {
        Ok(raw) => {
            let version: u32 = raw.trim().parse().map_err(|_| {
                PortError::InvalidState(format!(
                    "embedded store at `{}` has a corrupt FORMAT_VERSION (`{}`); refusing to open",
                    data_dir.display(),
                    raw.trim()
                ))
            })?;
            let stamped = resolve_stamped(data_dir, version)?;
            if let Some(wanted) = wanted
                && wanted != stamped
            {
                return Err(PortError::InvalidState(format!(
                    "embedded store at `{}` is a {stamped} store (format version {}), not {wanted}; \
                     a store is never reopened with another engine — to change engines, migrate it: \
                     `kmp-mcp migrate <this-dir> <new-dir> --engine {wanted}`, or unset the engine \
                     to open it as it is",
                    data_dir.display(),
                    stamped.format_version()
                )));
            }
            Ok(stamped)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            if any_store_file_exists(data_dir) {
                return Err(PortError::InvalidState(format!(
                    "embedded store at `{}` has a store file but no FORMAT_VERSION; the data \
                     directory layout is corrupt, refusing to open",
                    data_dir.display()
                )));
            }
            let engine = wanted.unwrap_or(StorageEngine::Sqlite);
            require_compiled(data_dir, engine)?;
            fs::write(&version_path, format!("{}\n", engine.format_version())).map_err(
                |error| {
                    PortError::Unavailable(format!(
                        "embedded store could not stamp FORMAT_VERSION at `{}`: {error}",
                        version_path.display()
                    ))
                },
            )?;
            Ok(engine)
        }
        Err(error) => Err(PortError::Unavailable(format!(
            "embedded store could not read FORMAT_VERSION at `{}`: {error}",
            version_path.display()
        ))),
    }
}

/// Maps a stamped number to an engine this build can open, or says exactly
/// why not: too old to migrate, too new for the binary, or known but not
/// compiled in.
fn resolve_stamped(data_dir: &Path, version: u32) -> Result<StorageEngine, PortError> {
    if version > StorageEngine::NEWEST_KNOWN_FORMAT_VERSION {
        return Err(PortError::InvalidState(format!(
            "embedded store at `{}` uses format version {version}, newer than this \
             binary supports ({}); upgrade the binary",
            data_dir.display(),
            StorageEngine::NEWEST_KNOWN_FORMAT_VERSION
        )));
    }
    let Some(engine) = StorageEngine::from_format_version(version) else {
        return Err(PortError::InvalidState(format!(
            "embedded store at `{}` uses format version {version}, older than this \
             binary supports ({SUPPORTED_FORMAT_VERSION}); migrate it with \
             `kmp-mcp migrate <this-dir> <new-dir>` — the source is left untouched",
            data_dir.display()
        )));
    };
    require_compiled(data_dir, engine)?;
    Ok(engine)
}

fn require_compiled(data_dir: &Path, engine: StorageEngine) -> Result<(), PortError> {
    if engine.is_compiled() {
        return Ok(());
    }
    Err(PortError::Unavailable(format!(
        "embedded store at `{}` uses the {engine} engine (format version {}), which this \
         binary was built without; rebuild with `--features {engine}`, or open it with a \
         build that has it",
        data_dir.display(),
        engine.format_version()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_directory_is_stamped_with_supported_version() {
        let dir = tempfile::tempdir().expect("tempdir");

        let engine = check_or_stamp(dir.path()).expect("fresh directory should stamp");

        assert_eq!(engine, StorageEngine::Sqlite);
        let stamped = fs::read_to_string(format_version_path(dir.path())).expect("read stamp");
        assert_eq!(stamped.trim(), SUPPORTED_FORMAT_VERSION.to_string());
        check_or_stamp(dir.path()).expect("stamped directory should reopen");
    }

    #[test]
    fn newer_format_version_fails_fast() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(format_version_path(dir.path()), "999\n").expect("write");

        let error = check_or_stamp(dir.path()).expect_err("newer version must fail");
        assert!(error.to_string().contains("upgrade the binary"));
    }

    #[test]
    fn older_format_version_requires_migration() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(format_version_path(dir.path()), "0\n").expect("write");

        let error = check_or_stamp(dir.path()).expect_err("older version must fail");
        assert!(error.to_string().contains("kmp-mcp migrate"));
    }

    #[test]
    fn corrupt_version_content_fails_fast() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(format_version_path(dir.path()), "not-a-number\n").expect("write");

        let error = check_or_stamp(dir.path()).expect_err("corrupt version must fail");
        assert!(error.to_string().contains("corrupt FORMAT_VERSION"));
    }

    #[test]
    fn store_without_version_stamp_is_a_corrupt_layout() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = store_file_path_for(dir.path(), StorageEngine::Redb);
        fs::create_dir_all(store.parent().expect("parent")).expect("mkdir");
        fs::write(&store, b"stub").expect("write store stub");

        let error = check_or_stamp(dir.path()).expect_err("missing stamp must fail");
        assert!(error.to_string().contains("corrupt"));
    }

    #[test]
    fn a_store_is_never_reopened_as_another_engine() {
        let dir = tempfile::tempdir().expect("tempdir");
        check_or_stamp_as(dir.path(), Some(StorageEngine::Redb)).expect("stamps legacy redb");

        let error = check_or_stamp_as(dir.path(), Some(StorageEngine::Sqlite))
            .expect_err("redb store must not open as sqlite");
        assert!(error.to_string().contains("is a redb store"));
        assert!(error.to_string().contains("not sqlite"));
    }

    #[test]
    fn sqlite_layout_is_always_available() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(format_version_path(dir.path()), "2\n").expect("write");

        assert_eq!(
            check_or_stamp(dir.path()).expect("sqlite is always compiled in"),
            StorageEngine::Sqlite
        );
    }
}
