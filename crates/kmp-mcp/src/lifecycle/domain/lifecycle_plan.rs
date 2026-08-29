use super::engine_install_dir::EngineInstallDir;
use super::host::Host;
use super::host_installation::HostInstallation;
use super::lifecycle_action::LifecycleAction;
use super::lifecycle_error::LifecycleError;
use super::lifecycle_request::LifecycleRequest;
use super::release_version::ReleaseVersion;

/// Immutable convergence decision. A host flag can add intent, never exclude
/// another enabled KMP consumer of the shared engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecyclePlan {
    action: LifecycleAction,
    hosts: Vec<Host>,
    target: ReleaseVersion,
    install_dir: EngineInstallDir,
    dry_run: bool,
}

impl LifecyclePlan {
    pub fn decide(
        request: &LifecycleRequest,
        installed: &[HostInstallation],
        available: &[Host],
        target: ReleaseVersion,
    ) -> Result<Self, LifecycleError> {
        let mut hosts = request.requested_hosts().clone();
        hosts.extend(
            installed
                .iter()
                .filter(|installation| installation.participates_in_convergence())
                .map(HostInstallation::host),
        );
        if request.action() == LifecycleAction::Setup && request.requested_hosts().is_empty() {
            hosts.extend(available.iter().copied());
        }
        if hosts.is_empty() {
            return Err(LifecycleError::NoInstalledHost);
        }
        let hosts = Host::CONVERGENCE_ORDER
            .into_iter()
            .filter(|host| hosts.contains(host))
            .collect();
        Ok(Self {
            action: request.action(),
            hosts,
            target,
            install_dir: request.install_dir().clone(),
            dry_run: request.is_dry_run(),
        })
    }

    pub fn action(&self) -> LifecycleAction {
        self.action
    }

    pub fn hosts(&self) -> &[Host] {
        &self.hosts
    }

    pub fn target(&self) -> &ReleaseVersion {
        &self.target
    }

    pub fn install_dir(&self) -> &EngineInstallDir {
        &self.install_dir
    }

    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::lifecycle::domain::plugin_root::PluginRoot;

    #[test]
    fn a_named_host_cannot_exclude_another_enabled_kmp_consumer() {
        let mut requested = BTreeSet::new();
        requested.insert(Host::Codex);
        let request = LifecycleRequest::new(
            LifecycleAction::Update,
            requested,
            Some(ReleaseVersion::parse("0.5.1").expect("version")),
            EngineInstallDir::new("/tmp/kmp-bin").expect("install dir"),
            false,
        );
        let installed = vec![HostInstallation::discovered(
            Host::Claude,
            ReleaseVersion::parse("0.4.2").expect("version"),
            PluginRoot::new("/tmp/claude-kmp").expect("root"),
            true,
        )];

        let plan = LifecyclePlan::decide(
            &request,
            &installed,
            &Host::CONVERGENCE_ORDER,
            ReleaseVersion::parse("0.5.1").expect("target"),
        )
        .expect("plan");
        assert_eq!(plan.hosts(), &[Host::Claude, Host::Codex]);
    }
}
