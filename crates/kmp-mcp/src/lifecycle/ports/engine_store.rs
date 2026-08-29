use crate::lifecycle::domain::engine_artifact::EngineArtifact;
use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::engine_install_dir::EngineInstallDir;
use crate::lifecycle::domain::engine_proof::EngineProof;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::domain::tree_digest::TreeDigest;

/// Outbound port for atomic executable installation and runtime proof.
pub trait EngineStore: Send + Sync {
    fn running_engine(
        &self,
        target: &ReleaseVersion,
    ) -> Result<Option<EngineArtifact>, LifecycleError>;

    fn install(
        &self,
        artifact: &EngineArtifact,
        destination: &EngineInstallDir,
    ) -> Result<EngineExecutable, LifecycleError>;

    /// Prove the complete binary surface in an isolated staging location
    /// before any host manager or shared engine is mutated.
    fn stage_and_prove(
        &self,
        artifact: &EngineArtifact,
        target: &ReleaseVersion,
    ) -> Result<EngineProof, LifecycleError>;

    fn prove(
        &self,
        executable: &EngineExecutable,
        target: &ReleaseVersion,
    ) -> Result<EngineProof, LifecycleError>;

    /// Exact marketplace payload digest. Host-manager lease markers are not
    /// payload; the installed engine is, and must therefore also match.
    fn digest_tree(&self, root: &PluginRoot) -> Result<TreeDigest, LifecycleError>;
}
