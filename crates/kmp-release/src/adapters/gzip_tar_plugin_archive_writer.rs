use std::path::Path;

use flate2::Compression;
use flate2::GzBuilder;

use crate::application::dto::plugin_archive_entry_dto::PluginArchiveEntryDto;
use crate::domain::release_error::ReleaseError;
use crate::ports::plugin_archive_writer::PluginArchiveWriter;

pub struct GzipTarPluginArchiveWriter;

impl PluginArchiveWriter for GzipTarPluginArchiveWriter {
    fn write(
        &self,
        destination: &Path,
        entries: &[PluginArchiveEntryDto],
    ) -> Result<(), ReleaseError> {
        let file = std::fs::File::create(destination)
            .map_err(|error| ReleaseError::io("create archive", destination, &error))?;
        let encoder = GzBuilder::new().mtime(0).write(file, Compression::best());
        let mut archive = tar::Builder::new(encoder);
        let mut ordered = entries.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| left.destination.cmp(&right.destination));
        for entry in ordered {
            let mut header = tar::Header::new_gnu();
            header.set_size(
                u64::try_from(entry.content.len())
                    .map_err(|_| ReleaseError::invalid("plugin archive entry is too large"))?,
            );
            header.set_mode(if entry.executable { 0o755 } else { 0o644 });
            header.set_uid(0);
            header.set_gid(0);
            header.set_mtime(0);
            header.set_cksum();
            archive
                .append_data(
                    &mut header,
                    entry.destination.as_str(),
                    entry.content.as_slice(),
                )
                .map_err(|error| {
                    ReleaseError::invalid(format!(
                        "could not add {} to plugin archive: {error}",
                        entry.destination
                    ))
                })?;
        }
        let encoder = archive.into_inner().map_err(|error| {
            ReleaseError::invalid(format!("could not finish plugin tar: {error}"))
        })?;
        encoder
            .finish()
            .map(|_| ())
            .map_err(|error| ReleaseError::io("finish plugin gzip", destination, &error))
    }
}
