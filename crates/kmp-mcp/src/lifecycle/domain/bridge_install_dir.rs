use std::path::{Path, PathBuf};

use serde::Serialize;

use super::lifecycle_error::LifecycleError;

/// Absolute directory into which one lexical-bridge table may be installed.
///
/// The machine's table lives beside the stores rather than inside one,
/// because a store is selected per working directory: a project `.kernel/`
/// wins over the user default, and copying several megabytes into every
/// project that ever opens memory is not a distribution.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct BridgeInstallDir(PathBuf);

impl BridgeInstallDir {
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

    pub fn table(&self) -> PathBuf {
        self.0.join("lexical-bridge.kmpb")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_sits_inside_the_directory() {
        let directory = BridgeInstallDir::new("/home/data/kmp").expect("absolute");

        assert_eq!(
            directory.table(),
            Path::new("/home/data/kmp/lexical-bridge.kmpb")
        );
    }

    #[test]
    fn a_relative_directory_is_refused_at_the_boundary() {
        assert!(BridgeInstallDir::new("kmp").is_err());
    }
}
