use std::path::{Path, PathBuf};
use std::sync::Mutex;

use kmp_mcp::lifecycle::domain::bridge_install_dir::BridgeInstallDir;
use kmp_mcp::lifecycle::domain::lexical_bridge_artifact::LexicalBridgeArtifact;
use kmp_mcp::lifecycle::domain::lifecycle_error::LifecycleError;
use kmp_mcp::lifecycle::ports::bridge_store::BridgeStore;

/// A table store that keeps its one table in memory.
#[derive(Default)]
pub struct FakeBridgeStore {
    installed: Mutex<Option<String>>,
    refuses: Option<String>,
}

impl FakeBridgeStore {
    /// A machine that already holds a table with this digest.
    pub fn holding(sha256: &str) -> Self {
        Self {
            installed: Mutex::new(Some(sha256.to_string())),
            refuses: None,
        }
    }

    /// A filesystem that will not accept the table.
    pub fn refusing(reason: &str) -> Self {
        Self {
            installed: Mutex::new(None),
            refuses: Some(reason.to_string()),
        }
    }

    pub fn installed_sha256(&self) -> Option<String> {
        self.installed
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

impl BridgeStore for FakeBridgeStore {
    fn installed_digest(&self, _destination: &BridgeInstallDir) -> Option<String> {
        self.installed_sha256()
    }

    fn read(&self, path: &Path) -> Result<LexicalBridgeArtifact, LifecycleError> {
        Ok(LexicalBridgeArtifact::verified(
            b"a table an operator built".to_vec(),
            "operator-digest".to_string(),
            path.display().to_string(),
        ))
    }

    fn install(
        &self,
        artifact: &LexicalBridgeArtifact,
        destination: &BridgeInstallDir,
    ) -> Result<PathBuf, LifecycleError> {
        if let Some(reason) = &self.refuses {
            return Err(LifecycleError::Io {
                path: destination.table(),
                detail: reason.clone(),
            });
        }
        *self
            .installed
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(artifact.sha256().to_string());
        Ok(destination.table())
    }
}
