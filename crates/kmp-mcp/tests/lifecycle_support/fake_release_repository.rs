use std::sync::Mutex;

use kmp_mcp::lifecycle::domain::engine_artifact::EngineArtifact;
use kmp_mcp::lifecycle::domain::lexical_bridge_artifact::LexicalBridgeArtifact;
use kmp_mcp::lifecycle::domain::lifecycle_error::LifecycleError;
use kmp_mcp::lifecycle::domain::release_version::ReleaseVersion;
use kmp_mcp::lifecycle::ports::release_repository::ReleaseRepository;

pub struct FakeReleaseRepository {
    release: ReleaseVersion,
    lexical_bridge: Option<(String, Vec<u8>)>,
    /// How many times the table itself was downloaded, so a test can prove
    /// the checksum decided before several megabytes moved.
    bridge_downloads: Mutex<usize>,
    /// Which releases were asked for a table, so a test can prove the
    /// convergence asks the release it is converging to.
    bridge_asked_for: Mutex<Vec<String>>,
}

impl FakeReleaseRepository {
    pub fn publishing(release: ReleaseVersion) -> Self {
        Self {
            release,
            lexical_bridge: None,
            bridge_downloads: Mutex::new(0),
            bridge_asked_for: Mutex::new(Vec::new()),
        }
    }

    /// A release that also publishes a lexical-bridge table.
    pub fn with_lexical_bridge(mut self, sha256: &str, bytes: Vec<u8>) -> Self {
        self.lexical_bridge = Some((sha256.to_string(), bytes));
        self
    }

    pub fn bridge_asked_for(&self) -> Vec<String> {
        self.bridge_asked_for
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn bridge_downloads(&self) -> usize {
        *self
            .bridge_downloads
            .lock()
            .unwrap_or_else(|error| error.into_inner())
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

    fn lexical_bridge_checksum(
        &self,
        version: &ReleaseVersion,
    ) -> Result<Option<String>, LifecycleError> {
        self.bridge_asked_for
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(version.tag());
        Ok(self
            .lexical_bridge
            .as_ref()
            .map(|(sha256, _)| sha256.clone()))
    }

    fn lexical_bridge(
        &self,
        version: &ReleaseVersion,
    ) -> Result<LexicalBridgeArtifact, LifecycleError> {
        let (sha256, bytes) = self.lexical_bridge.as_ref().ok_or_else(|| {
            LifecycleError::Network(format!("release {} publishes no table", version.tag()))
        })?;
        *self
            .bridge_downloads
            .lock()
            .unwrap_or_else(|error| error.into_inner()) += 1;
        Ok(LexicalBridgeArtifact::verified(
            bytes.clone(),
            sha256.clone(),
            "the fake release".to_string(),
        ))
    }
}
