use std::path::Path;

use crate::domain::release_error::ReleaseError;
use crate::ports::guide_engine::GuideEngine;

pub trait GuideEngineFactory {
    fn create(&self, binary: &Path) -> Result<Box<dyn GuideEngine>, ReleaseError>;
}
