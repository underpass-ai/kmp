use serde::Serialize;

use super::host::Host;
use super::lifecycle_error::LifecycleError;
use super::plugin_root::PluginRoot;
use super::release_version::ReleaseVersion;

/// Native plugin state owned by one host manager.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HostInstallation {
    host: Host,
    version: ReleaseVersion,
    root: PluginRoot,
    enabled: bool,
}

impl HostInstallation {
    pub fn discovered(
        host: Host,
        version: ReleaseVersion,
        root: PluginRoot,
        enabled: bool,
    ) -> Self {
        Self {
            host,
            version,
            root,
            enabled,
        }
    }

    pub fn host(&self) -> Host {
        self.host
    }

    pub fn root(&self) -> &PluginRoot {
        &self.root
    }

    pub fn version(&self) -> &ReleaseVersion {
        &self.version
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn participates_in_convergence(&self) -> bool {
        self.enabled
    }

    pub fn require_release(&self, target: &ReleaseVersion) -> Result<(), LifecycleError> {
        if target.represents_same_release(&self.version) {
            Ok(())
        } else {
            Err(LifecycleError::HostVersionMismatch(format!(
                "{} installed plugin {}, but lifecycle requires {}",
                self.host, self.version, target
            )))
        }
    }
}
