use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use kmp_domain::PortError;

use super::format_version::{SUPPORTED_FORMAT_VERSION, format_version_path, read_stamped_version};

static STAMP_ID: AtomicU64 = AtomicU64::new(0);

/// Fence older binaries before adding card events. A crash before adoption
/// leaves a supported format-4 store whose missing migration receipt causes
/// adoption to retry. The original SQLite file and source events stay intact.
pub(super) fn upgrade_stamp(data_dir: &Path) -> Result<(), PortError> {
    match read_stamped_version(data_dir)? {
        SUPPORTED_FORMAT_VERSION => return Ok(()),
        3 => {}
        version => {
            return Err(PortError::InvalidState(format!(
                "card history upgrade requires format 3, found {version}; the stamp was not changed"
            )));
        }
    }
    let temporary = data_dir.join(format!(
        ".FORMAT_VERSION-{}-{}",
        std::process::id(),
        STAMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        writeln!(file, "{SUPPORTED_FORMAT_VERSION}")?;
        file.sync_all()?;
        fs::rename(&temporary, format_version_path(data_dir))?;
        #[cfg(unix)]
        fs::File::open(data_dir)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| {
        PortError::Unavailable(format!("could not upgrade card history format: {error}"))
    })
}
