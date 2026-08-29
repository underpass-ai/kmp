use super::convergence_status::ConvergenceStatus;
use super::host::Host;
use super::host_installation::HostInstallation;
use super::lifecycle_action::LifecycleAction;
use super::plugin_root::PluginRoot;
use super::release_version::ReleaseVersion;

/// Lifecycle result for one host. It owns the distinction between observed,
/// planned and completed state so boundary DTOs cannot blur them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostConvergence {
    host: Host,
    previous: Option<HostInstallation>,
    current: Option<HostInstallation>,
    target: ReleaseVersion,
    status: ConvergenceStatus,
}

impl HostConvergence {
    pub fn planned(
        action: LifecycleAction,
        host: Host,
        previous: Option<HostInstallation>,
        target: ReleaseVersion,
    ) -> Self {
        let already_converged = previous.as_ref().is_some_and(|installation| {
            installation.participates_in_convergence()
                && installation.require_release(&target).is_ok()
        });
        let status = if action == LifecycleAction::Setup && already_converged {
            ConvergenceStatus::Unchanged
        } else {
            ConvergenceStatus::PlannedChange
        };
        Self {
            host,
            previous,
            current: None,
            target,
            status,
        }
    }

    pub fn completed(
        action: LifecycleAction,
        previous: Option<HostInstallation>,
        current: HostInstallation,
    ) -> Self {
        let status = if action == LifecycleAction::Setup
            && previous.as_ref().is_some_and(|before| before == &current)
        {
            ConvergenceStatus::Unchanged
        } else {
            ConvergenceStatus::Changed
        };
        Self {
            host: current.host(),
            previous,
            target: current.version().clone(),
            current: Some(current),
            status,
        }
    }

    pub fn host(&self) -> Host {
        self.host
    }

    pub fn previous_version(&self) -> Option<&ReleaseVersion> {
        self.previous.as_ref().map(HostInstallation::version)
    }

    pub fn version(&self) -> &ReleaseVersion {
        self.current
            .as_ref()
            .map(HostInstallation::version)
            .unwrap_or(&self.target)
    }

    pub fn root(&self) -> Option<&PluginRoot> {
        self.current
            .as_ref()
            .or(self.previous.as_ref())
            .map(HostInstallation::root)
    }

    pub fn is_enabled(&self) -> bool {
        self.current
            .as_ref()
            .or(self.previous.as_ref())
            .is_none_or(HostInstallation::is_enabled)
    }

    pub fn status(&self) -> ConvergenceStatus {
        self.status
    }
}
