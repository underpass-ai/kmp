use crate::lifecycle::domain::engine_artifact::EngineArtifact;
use crate::lifecycle::domain::lexical_bridge_artifact::LexicalBridgeArtifact;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::release_version::ReleaseVersion;

/// Outbound port for immutable public KMP release artifacts.
pub trait ReleaseRepository: Send + Sync {
    fn latest(&self) -> Result<ReleaseVersion, LifecycleError>;

    fn engine(&self, version: &ReleaseVersion) -> Result<EngineArtifact, LifecycleError>;

    /// The published digest of this release's lexical-bridge table, or none
    /// when the release publishes no table.
    ///
    /// The checksum is a separate call because the table is several
    /// megabytes and rarely changes: a machine that already holds the
    /// published digest downloads nothing.
    fn lexical_bridge_checksum(
        &self,
        version: &ReleaseVersion,
    ) -> Result<Option<String>, LifecycleError>;

    /// The table itself, verified against its published checksum.
    fn lexical_bridge(
        &self,
        version: &ReleaseVersion,
    ) -> Result<LexicalBridgeArtifact, LifecycleError>;
}
