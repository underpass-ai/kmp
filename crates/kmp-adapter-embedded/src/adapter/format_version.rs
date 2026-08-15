use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use kmp_domain::PortError;

/// Store format version this binary reads and writes.
pub const SUPPORTED_FORMAT_VERSION: u32 = 1;

const FORMAT_VERSION_FILE: &str = "FORMAT_VERSION";

pub fn format_version_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FORMAT_VERSION_FILE)
}

/// The version stamped in `data_dir`, without applying the gate.
///
/// The migration needs to know what it is looking at precisely when
/// [`check_or_stamp`] would refuse to open it.
pub(crate) fn read_stamped_version(data_dir: &Path) -> Result<u32, PortError> {
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

pub(crate) fn store_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("kernel.redb")
}

/// Fail-fast format check per ADR-012: stamp fresh directories, reject
/// version mismatches and half-initialized layouts explicitly — never open a
/// store that could silently read as empty memory.
pub(crate) fn check_or_stamp(data_dir: &Path) -> Result<(), PortError> {
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
            if version > SUPPORTED_FORMAT_VERSION {
                return Err(PortError::InvalidState(format!(
                    "embedded store at `{}` uses format version {version}, newer than this \
                     binary supports ({SUPPORTED_FORMAT_VERSION}); upgrade the binary",
                    data_dir.display()
                )));
            }
            if version < SUPPORTED_FORMAT_VERSION {
                return Err(PortError::InvalidState(format!(
                    "embedded store at `{}` uses format version {version}, older than this \
                     binary supports ({SUPPORTED_FORMAT_VERSION}); migrate it with \
                     `kmp-mcp migrate <this-dir> <new-dir>` — the source is left untouched",
                    data_dir.display()
                )));
            }
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            if store_file_path(data_dir).exists() {
                return Err(PortError::InvalidState(format!(
                    "embedded store at `{}` has a store file but no FORMAT_VERSION; the data \
                     directory layout is corrupt, refusing to open",
                    data_dir.display()
                )));
            }
            fs::write(&version_path, format!("{SUPPORTED_FORMAT_VERSION}\n")).map_err(|error| {
                PortError::Unavailable(format!(
                    "embedded store could not stamp FORMAT_VERSION at `{}`: {error}",
                    version_path.display()
                ))
            })
        }
        Err(error) => Err(PortError::Unavailable(format!(
            "embedded store could not read FORMAT_VERSION at `{}`: {error}",
            version_path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_directory_is_stamped_with_supported_version() {
        let dir = tempfile::tempdir().expect("tempdir");

        check_or_stamp(dir.path()).expect("fresh directory should stamp");

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
        let store = store_file_path(dir.path());
        fs::create_dir_all(store.parent().expect("parent")).expect("mkdir");
        fs::write(&store, b"stub").expect("write store stub");

        let error = check_or_stamp(dir.path()).expect_err("missing stamp must fail");
        assert!(error.to_string().contains("corrupt"));
    }
}
