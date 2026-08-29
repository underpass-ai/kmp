use std::path::Path;

use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::ports::release_file_system::ReleaseFileSystem;

pub struct CheckChangelog<'a, F> {
    file_system: &'a F,
}

impl<'a, F: ReleaseFileSystem> CheckChangelog<'a, F> {
    pub fn new(file_system: &'a F) -> Self {
        Self { file_system }
    }

    pub fn execute(&self, path: &Path, version: &ReleaseVersion) -> Result<(), ReleaseError> {
        let text = self.file_system.read_text(path)?;
        let heading = format!("## [{version}]");
        let start = text
            .lines()
            .position(|line| line == heading || line.starts_with(&format!("{heading} - ")))
            .ok_or_else(|| {
                ReleaseError::invalid(format!(
                    "{}: missing release section {heading}",
                    path.display()
                ))
            })?;
        let lines = text.lines().collect::<Vec<_>>();
        let end = lines[start + 1..]
            .iter()
            .position(|line| line.starts_with("## ["))
            .map_or(lines.len(), |offset| start + 1 + offset);
        if !lines[start + 1..end]
            .iter()
            .any(|line| line.starts_with("- ") && line.len() > 2)
        {
            return Err(ReleaseError::invalid(format!(
                "{}: [{version}] has no changelog entries",
                path.display()
            )));
        }
        let link = format!("[{version}]:");
        if !lines.iter().any(|line| {
            line.strip_prefix(&link)
                .is_some_and(|destination| !destination.trim().is_empty())
        }) {
            return Err(ReleaseError::invalid(format!(
                "{}: missing [{version}] comparison link",
                path.display()
            )));
        }
        Ok(())
    }
}
