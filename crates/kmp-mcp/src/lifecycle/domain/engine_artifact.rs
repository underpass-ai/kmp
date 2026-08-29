use super::lifecycle_error::LifecycleError;
use super::release_version::ReleaseVersion;

/// Checksum-verified engine bytes for one exact release.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineArtifact {
    version: ReleaseVersion,
    bytes: Vec<u8>,
}

impl EngineArtifact {
    pub fn verified(version: ReleaseVersion, bytes: Vec<u8>) -> Self {
        Self { version, bytes }
    }

    pub fn version(&self) -> &ReleaseVersion {
        &self.version
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn require_release(&self, target: &ReleaseVersion) -> Result<(), LifecycleError> {
        if target.represents_same_release(&self.version) {
            Ok(())
        } else {
            Err(LifecycleError::HostVersionMismatch(format!(
                "engine artifact {} does not represent requested release {}",
                self.version, target
            )))
        }
    }
}
