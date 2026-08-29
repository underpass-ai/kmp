use std::path::{Path, PathBuf};

use crate::domain::public_overview::PublicOverview;
use crate::domain::release_error::ReleaseError;
use crate::ports::release_file_system::ReleaseFileSystem;

pub struct SyncPublicReadme<'a, F> {
    file_system: &'a F,
}

impl<'a, F: ReleaseFileSystem> SyncPublicReadme<'a, F> {
    pub fn new(file_system: &'a F) -> Self {
        Self { file_system }
    }

    pub fn execute(&self, source: &Path, targets: &[PathBuf]) -> Result<usize, ReleaseError> {
        let source_text = self.file_system.read_text(source)?;
        let overview = PublicOverview::parse(&source_text)?;
        let mut changed = 0;
        for target in targets {
            let current = self.file_system.read_text(target)?;
            let expected = overview.render_into(&current)?;
            if current == expected {
                continue;
            }
            self.file_system.write_text(target, &expected)?;
            changed += 1;
        }
        Ok(changed)
    }
}
