use std::path::Path;

use crate::application::dto::plugin_archive_entry_dto::PluginArchiveEntryDto;
use crate::domain::release_error::ReleaseError;

pub trait PluginArchiveWriter {
    fn write(
        &self,
        destination: &Path,
        entries: &[PluginArchiveEntryDto],
    ) -> Result<(), ReleaseError>;
}
