//! Writing a bundle file: atomically replaced, or published once.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use kmp_domain::PortError;

static UNIQUE_FILE: AtomicU64 = AtomicU64::new(0);

/// Same-directory durable replacement, so a failed export leaves either the
/// previous complete bundle or the next complete bundle, never half a JSONL
/// stream. Unix rename replaces atomically; Windows keeps the previous file
/// beside it until the new one has taken the canonical name.
pub fn write_bundle_atomically(path: &Path, bundle: &str) -> Result<(), PortError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent).map_err(|error| {
            PortError::Unavailable(format!(
                "could not create bundle directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }
    let temp = path.with_file_name(unique_name("memory", "tmp"));
    let write_result = (|| -> Result<(), PortError> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|error| {
                PortError::Unavailable(format!(
                    "could not create temporary bundle `{}`: {error}",
                    temp.display()
                ))
            })?;
        file.write_all(bundle.as_bytes()).map_err(|error| {
            PortError::Unavailable(format!(
                "could not write temporary bundle `{}`: {error}",
                temp.display()
            ))
        })?;
        file.sync_all().map_err(|error| {
            PortError::Unavailable(format!(
                "could not make temporary bundle `{}` durable: {error}",
                temp.display()
            ))
        })?;
        replace_file(&temp, path).map_err(|error| {
            PortError::Unavailable(format!(
                "could not replace bundle `{}`: {error}",
                path.display()
            ))
        })?;
        sync_parent(parent)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    write_result
}

/// Publishes an immutable bundle without a check-then-replace race. The hard
/// link is an atomic create-if-absent operation on the same filesystem: two
/// snapshot creators can agree on existing content, but neither can replace
/// the other's recovery point.
pub fn write_bundle_if_absent(path: &Path, bundle: &str) -> Result<bool, PortError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent).map_err(|error| {
            PortError::Unavailable(format!(
                "could not create bundle directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }
    let staged = path.with_file_name(unique_name("snapshot", "tmp"));
    write_bundle_atomically(&staged, bundle)?;
    let linked = match fs::hard_link(&staged, path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(error) => {
            let _ = fs::remove_file(&staged);
            return Err(PortError::Unavailable(format!(
                "could not publish immutable bundle `{}`: {error}",
                path.display()
            )));
        }
    };
    fs::remove_file(&staged).map_err(|error| {
        PortError::Unavailable(format!(
            "could not remove staged bundle `{}`: {error}",
            staged.display()
        ))
    })?;
    sync_parent(parent)?;
    Ok(linked)
}

pub(crate) fn unique_name(prefix: &str, suffix: &str) -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos();
    let sequence = UNIQUE_FILE.fetch_add(1, Ordering::Relaxed);
    format!(
        ".{prefix}-{}-{time}-{sequence}.{suffix}",
        std::process::id()
    )
}

#[cfg(not(windows))]
fn replace_file(temp: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(temp, destination)
}

#[cfg(windows)]
fn replace_file(temp: &Path, destination: &Path) -> std::io::Result<()> {
    let previous = destination.with_file_name(unique_name("memory", "previous"));
    if destination.exists() {
        fs::rename(destination, &previous)?;
    }
    match fs::rename(temp, destination) {
        Ok(()) => {
            let _ = fs::remove_file(previous);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(previous, destination);
            Err(error)
        }
    }
}

#[cfg(unix)]
pub(crate) fn sync_parent(parent: Option<&Path>) -> Result<(), PortError> {
    let Some(parent) = parent else {
        return Ok(());
    };
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            PortError::Unavailable(format!(
                "could not make bundle directory `{}` durable: {error}",
                parent.display()
            ))
        })
}

#[cfg(not(unix))]
pub(crate) fn sync_parent(_parent: Option<&Path>) -> Result<(), PortError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_a_complete_bundle() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(".kmp/memory.jsonl");
        write_bundle_atomically(&path, "first\n").expect("first");
        write_bundle_atomically(&path, "second\n").expect("second");
        assert_eq!(fs::read_to_string(path).expect("read"), "second\n");
    }

    #[test]
    fn immutable_write_never_replaces_an_existing_recovery_point() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join(".kmp/snapshots/release.jsonl");
        assert!(write_bundle_if_absent(&path, "first\n").expect("created"));
        assert!(!write_bundle_if_absent(&path, "second\n").expect("exists"));
        assert_eq!(fs::read_to_string(path).expect("read"), "first\n");
    }
}
