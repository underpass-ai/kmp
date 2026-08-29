use kmp_mcp::lifecycle::domain::engine_artifact::EngineArtifact;
use kmp_mcp::lifecycle::domain::lifecycle_error::LifecycleError;
use kmp_mcp::lifecycle::domain::release_version::ReleaseVersion;
use kmp_mcp::lifecycle::ports::release_repository::ReleaseRepository;

pub struct FakeReleaseRepository {
    release: ReleaseVersion,
}

impl FakeReleaseRepository {
    pub fn publishing(release: ReleaseVersion) -> Self {
        Self { release }
    }
}

impl ReleaseRepository for FakeReleaseRepository {
    fn latest(&self) -> Result<ReleaseVersion, LifecycleError> {
        Ok(self.release.clone())
    }

    fn engine(&self, version: &ReleaseVersion) -> Result<EngineArtifact, LifecycleError> {
        Ok(EngineArtifact::verified(
            version.clone(),
            b"engine".to_vec(),
        ))
    }
}
