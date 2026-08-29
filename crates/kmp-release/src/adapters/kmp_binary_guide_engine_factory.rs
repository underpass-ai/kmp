use std::path::Path;

use crate::adapters::kmp_binary_guide_engine::KmpBinaryGuideEngine;
use crate::domain::release_error::ReleaseError;
use crate::ports::guide_engine::GuideEngine;
use crate::ports::guide_engine_factory::GuideEngineFactory;
use crate::ports::release_binary_version_reader::ReleaseBinaryVersionReader;

#[derive(Debug, Clone, Copy, Default)]
pub struct KmpBinaryGuideEngineFactory;

impl GuideEngineFactory for KmpBinaryGuideEngineFactory {
    fn create(&self, binary: &Path) -> Result<Box<dyn GuideEngine>, ReleaseError> {
        Ok(Box::new(KmpBinaryGuideEngine::new(binary)?))
    }
}

impl ReleaseBinaryVersionReader for KmpBinaryGuideEngineFactory {
    fn read_version(
        &self,
        binary: &std::path::Path,
    ) -> Result<crate::domain::release_version::ReleaseVersion, ReleaseError> {
        KmpBinaryGuideEngine::new(binary)?.version()
    }
}
