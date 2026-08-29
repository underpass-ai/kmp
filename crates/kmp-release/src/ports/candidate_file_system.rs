use std::path::{Path, PathBuf};

use crate::domain::release_error::ReleaseError;

pub trait CandidateFileSystem {
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, ReleaseError>;
    fn write_bytes(&self, path: &Path, content: &[u8]) -> Result<(), ReleaseError>;
    fn create_dir_all(&self, path: &Path) -> Result<(), ReleaseError>;
    fn remove_dir_all_if_present(&self, path: &Path) -> Result<(), ReleaseError>;
    fn copy_file(&self, source: &Path, destination: &Path) -> Result<(), ReleaseError>;
    fn walk_files(&self, root: &Path) -> Result<Vec<PathBuf>, ReleaseError>;
    fn file_size(&self, path: &Path) -> Result<u64, ReleaseError>;
    fn is_executable(&self, path: &Path) -> Result<bool, ReleaseError>;
}
