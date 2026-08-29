use std::path::{Path, PathBuf};

use serde::Serialize;

use super::lifecycle_error::LifecycleError;

/// Absolute root owned by one native plugin manager.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PluginRoot(PathBuf);

impl PluginRoot {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, LifecycleError> {
        let path = path.into();
        if !path.is_absolute() || path.parent().is_none() {
            return Err(LifecycleError::UnsafePath(path));
        }
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn engine_dir(&self) -> PathBuf {
        self.0.join("bin")
    }
}
