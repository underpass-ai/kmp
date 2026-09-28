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
    warning: Option<String>,
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
            warning: None,
        }
    }

    /// A host auto-detection found but left alone because the package that
    /// carries its connection is not installed, and nothing KMP runs can
    /// install it. The same entry serves a plan and a completed run.
    pub fn skipped_without_package(
        host: Host,
        package: &str,
        previous: Option<HostInstallation>,
        target: ReleaseVersion,
    ) -> Self {
        Self {
            host,
            previous,
            current: None,
            target,
            status: ConvergenceStatus::Skipped,
            warning: Some(format!(
                "{host} present but {package} not registered; run `underpass setup`"
            )),
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
            warning: None,
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
        if self.status == ConvergenceStatus::Skipped {
            return false;
        }
        self.current
            .as_ref()
            .or(self.previous.as_ref())
            .is_none_or(HostInstallation::is_enabled)
    }

    pub fn status(&self) -> ConvergenceStatus {
        self.status
    }

    /// Why this host needs attention even though the run succeeded.
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skipped_host_is_disabled_and_says_how_to_add_its_package() {
        let skipped = HostConvergence::skipped_without_package(
            Host::Pi,
            "underpass-pi",
            None,
            ReleaseVersion::current(),
        );

        assert_eq!(skipped.status(), ConvergenceStatus::Skipped);
        assert!(!skipped.is_enabled());
        assert_eq!(skipped.root(), None);
        assert_eq!(
            skipped.warning(),
            Some("pi present but underpass-pi not registered; run `underpass setup`")
        );
    }

    #[test]
    fn a_converged_host_carries_no_warning() {
        let planned = HostConvergence::planned(
            LifecycleAction::Setup,
            Host::Codex,
            None,
            ReleaseVersion::current(),
        );
        assert_eq!(planned.warning(), None);
    }
}
