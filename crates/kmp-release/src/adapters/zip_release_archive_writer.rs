use std::io::Write;
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, System, ZipWriter};

use crate::application::dto::mcpb_archive_entry_dto::McpbArchiveEntryDto;
use crate::domain::release_error::ReleaseError;
use crate::ports::release_archive_writer::ReleaseArchiveWriter;

pub struct ZipReleaseArchiveWriter;

impl ReleaseArchiveWriter for ZipReleaseArchiveWriter {
    fn write(
        &self,
        destination: &Path,
        entries: &[McpbArchiveEntryDto],
    ) -> Result<(), ReleaseError> {
        let file = std::fs::File::create(destination)
            .map_err(|error| ReleaseError::io("create archive", destination, &error))?;
        let mut archive = ZipWriter::new(file);
        let mut ordered = entries.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| left.destination.cmp(&right.destination));
        for entry in ordered {
            let options = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .compression_level(Some(9))
                .system(System::Unix)
                .unix_permissions(if entry.executable { 0o755 } else { 0o644 });
            archive
                .start_file(entry.destination.as_str(), options)
                .map_err(|error| {
                    ReleaseError::invalid(format!(
                        "could not add {} to MCPB: {error}",
                        entry.destination
                    ))
                })?;
            let content = std::fs::read(&entry.source)
                .map_err(|error| ReleaseError::io("read", &entry.source, &error))?;
            archive
                .write_all(&content)
                .map_err(|error| ReleaseError::io("write archive entry", destination, &error))?;
        }
        archive.finish().map(|_| ()).map_err(|error| {
            ReleaseError::invalid(format!(
                "could not finish MCPB `{}`: {error}",
                destination.display()
            ))
        })
    }
}
