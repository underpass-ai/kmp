use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use kmp_domain::PortError;

/// The layout this binary creates for a fresh data directory: shareable
/// SQLite ([historical ADR-018](https://github.com/underpass-ai/kmp/blob/v0.5.0/archive/docs/adr/ADR-018-multi-process-embedded-store.md)).
///
/// `FORMAT_VERSION` in a data directory names the *layout* — which engine
/// wrote `store/`, and how. Bumping it is what makes a binary that predates
/// a layout refuse the directory instead of opening an empty store beside
/// it, so a new engine is a new number ([historical ADR-018](https://github.com/underpass-ai/kmp/blob/v0.5.0/archive/docs/adr/ADR-018-multi-process-embedded-store.md)).
pub const SUPPORTED_FORMAT_VERSION: u32 = StorageEngine::Sqlite.format_version();

/// Layout number used by the removed redb backend. Kept only so current
/// binaries can identify legacy memory and refuse it without touching it.
pub const LEGACY_REDB_FORMAT_VERSION: u32 = 1;

/// The logical shape of the event log carried by a portable bundle.
pub const EVENT_FORMAT_VERSION: u32 = 1;

const FORMAT_VERSION_FILE: &str = "FORMAT_VERSION";

/// The engine behind a data directory's `store/`. Chosen once, when the
/// directory is created; recorded as its `FORMAT_VERSION`; never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageEngine {
    /// WAL-mode SQLite: several processes may open the same store. This is
    /// the only compiled storage engine.
    Sqlite,
}

impl StorageEngine {
    /// The `FORMAT_VERSION` this engine stamps.
    pub const fn format_version(self) -> u32 {
        match self {
            StorageEngine::Sqlite => 2,
        }
    }

    /// The highest layout number any build of this crate knows about,
    /// compiled in or not. Above this the binary is simply too old.
    pub(crate) const NEWEST_KNOWN_FORMAT_VERSION: u32 = StorageEngine::Sqlite.format_version();

    pub(crate) const fn from_format_version(version: u32) -> Option<Self> {
        match version {
            2 => Some(StorageEngine::Sqlite),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            StorageEngine::Sqlite => "sqlite",
        }
    }

    const fn store_file_name(self) -> &'static str {
        match self {
            StorageEngine::Sqlite => "kernel.sqlite3",
        }
    }
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

/// Historical format-1 store location. Current binaries never open this
/// file; the path is exposed only for diagnostics and fail-fast detection.
pub fn legacy_redb_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("kernel.redb")
}

/// Whether any engine's store file is present — the "half-initialized
/// layout" signal used to refuse a directory with a store but no stamp.
fn any_store_file_exists(data_dir: &Path) -> bool {
    store_file_path_for(data_dir, StorageEngine::Sqlite).exists()
        || legacy_redb_store_path(data_dir).exists()
}

/// Applies the existing-layout gate without creating or opening anything.
///
/// Diagnostics use this exact gate so they cannot call a store healthy when
/// the next real kernel operation will refuse it. `None` means genuinely
/// fresh: no stamp and no engine file. A stamp without a store file is also
/// valid — startup may have stopped between stamping and first engine open.
pub fn validate_store_layout(data_dir: &Path) -> Result<Option<StorageEngine>, PortError> {
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
            let sqlite_present = store_file_path_for(data_dir, StorageEngine::Sqlite).exists();
            let legacy_present = legacy_redb_store_path(data_dir).exists();
            if sqlite_present && legacy_present {
                return Err(PortError::InvalidState(format!(
                    "embedded store at `{}` contains multiple engine files (sqlite, redb); refusing to pick one",
                    data_dir.display()
                )));
            }
            if legacy_present {
                return Err(PortError::InvalidState(format!(
                    "embedded store at `{}` says format version {} ({stamped}), but its store file is redb; refusing to open memory under the wrong engine",
                    data_dir.display(),
                    stamped.format_version()
                )));
            }
            Ok(Some(stamped))
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            if any_store_file_exists(data_dir) {
                return Err(PortError::InvalidState(format!(
                    "embedded store at `{}` has a store file but no FORMAT_VERSION; the data \
                     directory layout is corrupt, refusing to open",
                    data_dir.display()
                )));
            }
            Ok(None)
        }
        Err(error) => Err(PortError::Unavailable(format!(
            "embedded store could not read FORMAT_VERSION at `{}`: {error}",
            version_path.display()
        ))),
    }
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
    match validate_store_layout(data_dir)? {
        Some(stamped) => {
            if let Some(wanted) = wanted
                && wanted != stamped
            {
                return Err(PortError::InvalidState(format!(
                    "embedded store at `{}` is a {stamped} store (format version {}), not {wanted}; \
                     a store is never reopened with another engine — to change engines, migrate it: \
                     `kmp-mcp migrate <this-dir> <new-dir>`, or unset the engine \
                     to open it as it is",
                    data_dir.display(),
                    stamped.format_version()
                )));
            }
            Ok(stamped)
        }
        None => {
            let engine = wanted.unwrap_or(StorageEngine::Sqlite);
            let version_path = format_version_path(data_dir);
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
    }
}

/// Maps a stamped number to an engine this build can open, or says exactly
/// why not: retired, unsupported, or too new for the binary.
fn resolve_stamped(data_dir: &Path, version: u32) -> Result<StorageEngine, PortError> {
    if version > StorageEngine::NEWEST_KNOWN_FORMAT_VERSION {
        return Err(PortError::InvalidState(format!(
            "embedded store at `{}` uses format version {version}, newer than this \
             binary supports ({}); upgrade the binary",
            data_dir.display(),
            StorageEngine::NEWEST_KNOWN_FORMAT_VERSION
        )));
    }
    if version == LEGACY_REDB_FORMAT_VERSION {
        return Err(PortError::InvalidState(format!(
            "embedded store at `{}` uses retired format 1 (redb); this binary contains no \
             redb reader and left the store untouched. Use KMP 0.3.2 to export \
             `.kmp/memory.jsonl`, then import that bundle with the current KMP",
            data_dir.display()
        )));
    }
    StorageEngine::from_format_version(version).ok_or_else(|| {
        PortError::InvalidState(format!(
            "embedded store at `{}` uses unsupported format version {version}; this binary \
             only opens format {SUPPORTED_FORMAT_VERSION} and left the store untouched",
            data_dir.display()
        ))
    })
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
    fn unknown_older_format_version_is_rejected_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(format_version_path(dir.path()), "0\n").expect("write");

        let error = check_or_stamp(dir.path()).expect_err("older version must fail");
        let message = error.to_string();
        assert!(
            message.contains("unsupported format version 0"),
            "{message}"
        );
        assert!(message.contains("left the store untouched"), "{message}");
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
        let store = legacy_redb_store_path(dir.path());
        fs::create_dir_all(store.parent().expect("parent")).expect("mkdir");
        fs::write(&store, b"stub").expect("write store stub");

        let error = check_or_stamp(dir.path()).expect_err("missing stamp must fail");
        assert!(error.to_string().contains("corrupt"));
    }

    #[test]
    fn diagnostics_can_apply_the_open_gate_without_stamping_a_fresh_directory() {
        let fresh = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            validate_store_layout(fresh.path()).expect("fresh layout is valid"),
            None
        );
        assert!(!format_version_path(fresh.path()).exists());

        let invalid = tempfile::tempdir().expect("tempdir");
        let store = store_file_path_for(invalid.path(), StorageEngine::Sqlite);
        fs::create_dir_all(store.parent().expect("parent")).expect("mkdir");
        fs::write(&store, b"memory remains here").expect("store marker");
        for stamp in [Some("3\n"), Some("banana\n"), None] {
            match stamp {
                Some(stamp) => fs::write(format_version_path(invalid.path()), stamp)
                    .expect("write invalid stamp"),
                None => fs::remove_file(format_version_path(invalid.path())).expect("remove stamp"),
            }
            let error = validate_store_layout(invalid.path())
                .expect_err("the same gate as real open must refuse this layout");
            let message = error.to_string();
            assert!(
                message.contains("upgrade the binary")
                    || message.contains("corrupt FORMAT_VERSION")
                    || message.contains("store file but no FORMAT_VERSION"),
                "{message}"
            );
            assert!(
                store.exists(),
                "the read-only probe preserves the memory file"
            );
        }
    }

    #[test]
    fn a_stamp_cannot_hide_a_store_from_another_engine() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(format_version_path(dir.path()), "2\n").expect("sqlite stamp");
        let redb = legacy_redb_store_path(dir.path());
        fs::create_dir_all(redb.parent().expect("parent")).expect("mkdir");
        fs::write(redb, b"legacy memory").expect("redb marker");

        let error = check_or_stamp(dir.path()).expect_err("mismatched engine must fail");
        assert!(error.to_string().contains("store file is redb"), "{error}");
    }

    #[test]
    fn a_legacy_redb_store_is_rejected_without_being_opened() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(format_version_path(dir.path()), "1\n").expect("legacy stamp");
        let store = legacy_redb_store_path(dir.path());
        fs::create_dir_all(store.parent().expect("parent")).expect("store dir");
        fs::write(&store, b"legacy bytes").expect("legacy bytes");

        let error = check_or_stamp(dir.path()).expect_err("redb must not open");
        let message = error.to_string();
        assert!(message.contains("contains no redb reader"), "{message}");
        assert!(message.contains("KMP 0.3.2"), "{message}");
        assert_eq!(fs::read(&store).expect("source remains"), b"legacy bytes");
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
