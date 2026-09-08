use std::path::Component;

use crate::application::dto::guide_source_dto::GuideSourceDto;
use crate::domain::release_error::ReleaseError;
use crate::domain::repository_root::RepositoryRoot;
use crate::ports::release_file_system::ReleaseFileSystem;

pub struct LoadGuideEditorial<'a, F> {
    file_system: &'a F,
}

impl<'a, F: ReleaseFileSystem> LoadGuideEditorial<'a, F> {
    pub fn new(file_system: &'a F) -> Self {
        Self { file_system }
    }

    pub fn execute(&self, root: &RepositoryRoot) -> Result<GuideSourceDto, ReleaseError> {
        let directory = root.join("plugins/kmp/guide");
        let mut source: GuideSourceDto = serde_json::from_str(
            &self
                .file_system
                .read_text(&directory.join("editorial.json"))?,
        )
        .map_err(|error| ReleaseError::invalid(format!("editorial.json is invalid: {error}")))?;
        for about in &mut source.abouts {
            for entry in &mut about.entries {
                if let Some(path) = &entry.text_file {
                    if !entry.text.is_empty()
                        || path.as_os_str().is_empty()
                        || path
                            .components()
                            .any(|part| !matches!(part, Component::Normal(_)))
                    {
                        return Err(ReleaseError::invalid(format!(
                            "guide entry `{}` needs either inline text or a relative text_file inside the guide directory",
                            entry.id
                        )));
                    }
                    entry.text = self.file_system.read_text(&directory.join(path))?;
                }
                if entry.text.trim().is_empty() {
                    return Err(ReleaseError::invalid(format!(
                        "guide entry `{}` has no text",
                        entry.id
                    )));
                }
            }
        }
        Ok(source)
    }
}
