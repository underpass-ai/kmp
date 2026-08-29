use std::collections::BTreeSet;

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
        }
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
}
