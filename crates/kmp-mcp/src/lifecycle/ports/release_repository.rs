use crate::lifecycle::domain::engine_artifact::EngineArtifact;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::release_version::ReleaseVersion;

/// Outbound port for immutable public KMP release artifacts.
pub trait ReleaseRepository: Send + Sync {
    fn latest(&self) -> Result<ReleaseVersion, LifecycleError>;

    fn engine(&self, version: &ReleaseVersion) -> Result<EngineArtifact, LifecycleError>;
}
