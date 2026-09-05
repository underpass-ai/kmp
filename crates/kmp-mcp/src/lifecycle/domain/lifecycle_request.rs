use std::collections::BTreeSet;

use super::bridge_choice::BridgeChoice;
use super::bridge_install_dir::BridgeInstallDir;
use super::engine_install_dir::EngineInstallDir;
use super::host::Host;
use super::lifecycle_action::LifecycleAction;
use super::release_version::ReleaseVersion;

/// Validated lifecycle intent entering the application layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleRequest {
    action: LifecycleAction,
    requested_hosts: BTreeSet<Host>,
    target: Option<ReleaseVersion>,
    install_dir: EngineInstallDir,
    dry_run: bool,
    bridge: BridgeChoice,
    bridge_dir: Option<BridgeInstallDir>,
}

impl LifecycleRequest {
    pub fn new(
        action: LifecycleAction,
        requested_hosts: BTreeSet<Host>,
        target: Option<ReleaseVersion>,
        install_dir: EngineInstallDir,
        dry_run: bool,
    ) -> Self {
        Self {
            action,
            requested_hosts,
            target,
            install_dir,
            dry_run,
            bridge: BridgeChoice::default(),
            bridge_dir: None,
        }
    }

    /// What to do about the lexical-bridge table, and where the machine keeps
    /// it. A request that never says has nowhere to put one and installs
    /// none, which is why this is a separate sentence rather than two more
    /// arguments a caller could get in the wrong order.
    pub fn with_bridge(
        mut self,
        bridge: BridgeChoice,
        bridge_dir: Option<BridgeInstallDir>,
    ) -> Self {
        self.bridge = bridge;
        self.bridge_dir = bridge_dir;
        self
    }

    pub fn action(&self) -> LifecycleAction {
        self.action
    }

    pub fn requested_hosts(&self) -> &BTreeSet<Host> {
        &self.requested_hosts
    }

    pub fn target(&self) -> Option<&ReleaseVersion> {
        self.target.as_ref()
    }

    pub fn install_dir(&self) -> &EngineInstallDir {
        &self.install_dir
    }

    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    pub fn bridge(&self) -> &BridgeChoice {
        &self.bridge
    }

    /// Where the machine's table goes, or none on a platform with no data
    /// home to put it in.
    pub fn bridge_dir(&self) -> Option<&BridgeInstallDir> {
        self.bridge_dir.as_ref()
    }
}
