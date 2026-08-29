use crate::domain::release_archive_path::ReleaseArchivePath;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PluginArchiveEntryDto {
    pub destination: ReleaseArchivePath,
    pub content: Vec<u8>,
    pub executable: bool,
}
