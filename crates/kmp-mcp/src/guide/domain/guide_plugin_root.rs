use std::path::{Path, PathBuf};

use crate::guide::domain::guide_error::GuideError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuidePluginRoot(PathBuf);

impl GuidePluginRoot {
    pub fn parse(path: impl Into<PathBuf>) -> Result<Self, GuideError> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Err(GuideError::invalid("guide plugin root cannot be empty"));
        }
        Ok(Self(path))
    }

    pub fn requests_path(&self) -> PathBuf {
        self.0.join("guide/guide.requests.json")
    }

    pub fn bundle_path(&self) -> PathBuf {
        self.0.join("guide/memory.jsonl")
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}
