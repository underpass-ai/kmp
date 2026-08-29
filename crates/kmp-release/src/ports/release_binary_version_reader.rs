use std::path::Path;

use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;

pub trait ReleaseBinaryVersionReader {
    fn read_version(&self, binary: &Path) -> Result<ReleaseVersion, ReleaseError>;
}
