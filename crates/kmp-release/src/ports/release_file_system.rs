use std::path::Path;

use crate::domain::release_error::ReleaseError;

pub trait ReleaseFileSystem {
    fn read_text(&self, path: &Path) -> Result<String, ReleaseError>;
    fn write_text(&self, path: &Path, content: &str) -> Result<(), ReleaseError>;
}
