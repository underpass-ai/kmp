use std::path::PathBuf;

use crate::lifecycle::domain::bridge_install_dir::BridgeInstallDir;
use crate::lifecycle::domain::lexical_bridge_artifact::LexicalBridgeArtifact;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;

/// Outbound port for the machine's lexical-bridge table.
pub trait BridgeStore: Send + Sync {
    /// The digest of the table already installed, or none. Comparing this
    /// against a published checksum is what keeps a second `setup` from
    /// downloading several megabytes to write the bytes that are there.
    fn installed_digest(&self, destination: &BridgeInstallDir) -> Option<String>;

    /// Read a table an operator built, computing its digest.
    fn read(&self, path: &std::path::Path) -> Result<LexicalBridgeArtifact, LifecycleError>;

    /// Write the table atomically, having first proved it parses. Installing
    /// bytes the kernel would refuse to read is worse than installing none:
    /// the machine looks equipped and `ask` still matches within one
    /// language.
    fn install(
        &self,
        artifact: &LexicalBridgeArtifact,
        destination: &BridgeInstallDir,
    ) -> Result<PathBuf, LifecycleError>;
}
