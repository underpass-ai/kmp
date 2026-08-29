use std::path::Path;

use crate::application::dto::mcpb_archive_entry_dto::McpbArchiveEntryDto;
use crate::domain::release_error::ReleaseError;

pub trait ReleaseArchiveWriter {
    fn write(
        &self,
        destination: &Path,
        entries: &[McpbArchiveEntryDto],
    ) -> Result<(), ReleaseError>;
}
