use crate::lifecycle::domain::engine_install_dir::EngineInstallDir;
use crate::lifecycle::domain::host_convergence::HostConvergence;
use crate::lifecycle::domain::host_engine_proof::HostEngineProof;
use crate::lifecycle::domain::host_installation::HostInstallation;
use crate::lifecycle::domain::lifecycle_action::LifecycleAction;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::lifecycle_plan::LifecyclePlan;
use crate::lifecycle::domain::lifecycle_receipt::LifecycleReceipt;
use crate::lifecycle::domain::lifecycle_request::LifecycleRequest;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::engine_store::EngineStore;
use crate::lifecycle::ports::host_gateway::HostGateway;
use crate::lifecycle::ports::release_repository::ReleaseRepository;

/// Shared application orchestration behind the setup and update use cases.
pub(super) struct ConvergeLifecycle<'a> {
    hosts: &'a dyn HostGateway,
    releases: &'a dyn ReleaseRepository,
    engines: &'a dyn EngineStore,
}

impl<'a> ConvergeLifecycle<'a> {
    pub fn new(
        hosts: &'a dyn HostGateway,
        releases: &'a dyn ReleaseRepository,
        engines: &'a dyn EngineStore,
    ) -> Self {
        Self {
            hosts,
            releases,
            engines,
        }
    }

    pub fn execute(&self, request: LifecycleRequest) -> Result<LifecycleReceipt, LifecycleError> {
        let installed = self.hosts.inventory()?;
        let target = self.target_for(&request)?;
        let available = self.hosts.available_hosts();
        let plan = LifecyclePlan::decide(&request, &installed, &available, target)?;
        if plan.is_dry_run() {
            let selected = plan
                .hosts()
                .iter()
                .map(|host| {
                    let previous = installed
                        .iter()
                        .find(|installation| installation.host() == *host)
                        .cloned();
                    HostConvergence::planned(plan.action(), *host, previous, plan.target().clone())
                })
                .collect();
            return Ok(LifecycleReceipt::planned(
                plan.action(),
                plan.target().clone(),
                selected,
            ));
        }

        let artifact = match self.engines.running_engine(plan.target())? {
            Some(artifact) => artifact,
            None => self.releases.engine(plan.target())?,
        };
        artifact.require_release(plan.target())?;
        self.engines.stage_and_prove(&artifact, plan.target())?;

        let mut converged = Vec::new();
        let mut host_results = Vec::new();
        let mut proofs = Vec::new();
        for host in plan.hosts() {
            let previous = installed
                .iter()
                .find(|installation| installation.host() == *host)
                .cloned();
            let installation = match plan.action() {
                LifecycleAction::Update => self.hosts.refresh(*host, plan.target())?,
                LifecycleAction::Setup => match previous.as_ref() {
                    Some(installation)
                        if installation.participates_in_convergence()
                            && installation.require_release(plan.target()).is_ok() =>
                    {
                        installation.clone()
                    }
                    Some(_) => self.hosts.refresh(*host, plan.target())?,
                    None => self.hosts.provision(*host, plan.target())?,
                },
            };
            installation.require_release(plan.target())?;
            if host.owns_plugin_engine() {
                let destination = EngineInstallDir::new(installation.root().engine_dir())?;
                let executable = self.engines.install(&artifact, &destination)?;
                proofs.push(HostEngineProof::new(
                    *host,
                    self.engines.prove(&executable, plan.target())?,
                ));
            }
            host_results.push(HostConvergence::completed(
                plan.action(),
                previous,
                installation.clone(),
            ));
            converged.push(installation);
        }

        let plugin_tree = self.require_equal_plugin_trees(&converged)?;

        // PATH is the final mutation only when a converged host actually
        // consumes it. If a prior gate failed, the previous shared engine is
        // untouched; a Claude-only plan never mutates it at all.
        if let Some(shared_consumer) = converged
            .iter()
            .find(|installation| !installation.host().owns_plugin_engine())
        {
            let primary = self.engines.install(&artifact, plan.install_dir())?;
            proofs.push(HostEngineProof::new(
                shared_consumer.host(),
                self.engines.prove(&primary, plan.target())?,
            ));
        }

        Ok(LifecycleReceipt::completed(
            plan.action(),
            plan.target().clone(),
            host_results,
            proofs,
            plugin_tree,
        ))
    }

    fn target_for(&self, request: &LifecycleRequest) -> Result<ReleaseVersion, LifecycleError> {
        match request.target() {
            Some(version) => Ok(version.clone()),
            None if request.action() == LifecycleAction::Setup => Ok(ReleaseVersion::current()),
            None => self.releases.latest(),
        }
    }

    fn require_equal_plugin_trees(
        &self,
        installations: &[HostInstallation],
    ) -> Result<Option<crate::lifecycle::domain::tree_digest::TreeDigest>, LifecycleError> {
        let mut digests = installations
            .iter()
            .map(|installation| self.engines.digest_tree(installation.root()))
            .collect::<Result<Vec<_>, _>>()?;
        let first = digests.pop();
        if let Some(expected) = first.as_ref()
            && digests.iter().any(|digest| digest != expected)
        {
            return Err(LifecycleError::TreeMismatch(
                "Codex and Claude Code installed different KMP plugin trees".to_string(),
            ));
        }
        Ok(first)
    }
}
