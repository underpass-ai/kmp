use std::path::{Path, PathBuf};

use crate::domain::release_error::ReleaseError;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RepositoryRoot(PathBuf);

impl RepositoryRoot {
    pub fn discover() -> Result<Self, ReleaseError> {
        let current = std::env::current_dir().map_err(|error| {
            ReleaseError::invalid(format!("cannot resolve current directory: {error}"))
        })?;
        for root in current.ancestors() {
            if root.join("Cargo.toml").is_file()
                && root.join("crates/kmp-release/Cargo.toml").is_file()
            {
                return Ok(Self(root.to_path_buf()));
            }
        }
        Err(ReleaseError::invalid(format!(
            "no KMP workspace found from `{}`; run inside a KMP checkout or provide explicit paths",
            current.display()
        )))
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
