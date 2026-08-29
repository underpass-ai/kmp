use std::path::{Path, PathBuf};

use crate::domain::release_error::ReleaseError;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RepositoryRoot(PathBuf);

impl RepositoryRoot {
    pub fn discover() -> Result<Self, ReleaseError> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| ReleaseError::invalid("kmp-release is not inside the KMP workspace"))?
            .to_path_buf();
        Ok(Self(root))
    }

    pub fn from_path(path: impl Into<PathBuf>) -> Result<Self, ReleaseError> {
        let path = path.into();
        if !path.is_dir() {
            return Err(ReleaseError::invalid(format!(
                "repository root `{}` is not a directory",
                path.display()
            )));
        }
        Ok(Self(path))
    }

    pub fn join(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.0.join(relative)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}
