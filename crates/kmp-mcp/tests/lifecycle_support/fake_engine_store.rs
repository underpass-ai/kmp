use std::path::PathBuf;
use std::sync::Mutex;

use kmp_mcp::lifecycle::domain::engine_artifact::EngineArtifact;
use kmp_mcp::lifecycle::domain::engine_executable::EngineExecutable;
use kmp_mcp::lifecycle::domain::engine_install_dir::EngineInstallDir;
use kmp_mcp::lifecycle::domain::engine_proof::EngineProof;
use kmp_mcp::lifecycle::domain::lifecycle_error::LifecycleError;
use kmp_mcp::lifecycle::domain::plugin_root::PluginRoot;
use kmp_mcp::lifecycle::domain::release_version::ReleaseVersion;
use kmp_mcp::lifecycle::domain::tree_digest::TreeDigest;
use kmp_mcp::lifecycle::ports::engine_store::EngineStore;

pub struct FakeEngineStore {
    running: Option<EngineArtifact>,
    installations: Mutex<Vec<PathBuf>>,
    divergent_trees: bool,
    rejected_stage: bool,
    staged: Mutex<usize>,
}

impl FakeEngineStore {
    pub fn empty() -> Self {
        Self {
            running: None,
            installations: Mutex::new(Vec::new()),
            divergent_trees: false,
            rejected_stage: false,
            staged: Mutex::new(0),
        }
    }

    pub fn running(artifact: EngineArtifact) -> Self {
        Self {
            running: Some(artifact),
            installations: Mutex::new(Vec::new()),
            divergent_trees: false,
            rejected_stage: false,
            staged: Mutex::new(0),
        }
    }

    pub fn with_divergent_trees(mut self) -> Self {
        self.divergent_trees = true;
        self
    }

    pub fn with_rejected_stage(mut self) -> Self {
        self.rejected_stage = true;
        self
    }

    pub fn staged_count(&self) -> usize {
        *self.staged.lock().expect("stage lock")
    }

    pub fn installations(&self) -> Vec<PathBuf> {
        self.installations
            .lock()
            .expect("installation lock")
            .clone()
    }
}

impl EngineStore for FakeEngineStore {
    fn running_engine(
        &self,
        _target: &ReleaseVersion,
    ) -> Result<Option<EngineArtifact>, LifecycleError> {
        Ok(self.running.clone())
    }

    fn install(
        &self,
        _artifact: &EngineArtifact,
        destination: &EngineInstallDir,
    ) -> Result<EngineExecutable, LifecycleError> {
        self.installations
            .lock()
            .expect("installation lock")
            .push(destination.as_path().to_path_buf());
        Ok(EngineExecutable::installed_at(destination.executable()))
    }

    fn stage_and_prove(
        &self,
        _artifact: &EngineArtifact,
        target: &ReleaseVersion,
    ) -> Result<EngineProof, LifecycleError> {
        *self.staged.lock().expect("stage lock") += 1;
        if self.rejected_stage {
            return Err(LifecycleError::SurfaceMismatch(
                "staged engine failed its black-box proof".to_string(),
            ));
        }
        Ok(EngineProof::proven(
            EngineExecutable::installed_at(PathBuf::from("/tmp/staged/kmp-mcp")),
            target.clone(),
            kmp_mcp::tool_names(),
        ))
    }

    fn prove(
        &self,
        executable: &EngineExecutable,
        target: &ReleaseVersion,
    ) -> Result<EngineProof, LifecycleError> {
        Ok(EngineProof::proven(
            executable.clone(),
            target.clone(),
            kmp_mcp::tool_names(),
        ))
    }

    fn digest_tree(&self, root: &PluginRoot) -> Result<TreeDigest, LifecycleError> {
        let digest = if self.divergent_trees && root.as_path().ends_with("codex") {
            "b"
        } else {
            "a"
        };
        Ok(TreeDigest::sha256(digest.repeat(64)))
    }
}
