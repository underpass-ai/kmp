use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// The committed file as this process left it: its length and modification
/// time. Anyone else who rewrites it moves one or the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileStamp {
    len: u64,
    modified: SystemTime,
}

impl FileStamp {
    pub(crate) fn of(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok()?,
        })
    }
}
