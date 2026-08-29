use std::path::{Path, PathBuf};

use serde::Serialize;

/// One installed KMP engine executable.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct EngineExecutable(PathBuf);

impl EngineExecutable {
    pub fn installed_at(path: PathBuf) -> Self {
        Self(path)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}
