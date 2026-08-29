use std::path::PathBuf;

use crate::domain::release_archive_path::ReleaseArchivePath;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct McpbArchiveEntryDto {
    pub source: PathBuf,
    pub destination: ReleaseArchivePath,
    pub executable: bool,
}
