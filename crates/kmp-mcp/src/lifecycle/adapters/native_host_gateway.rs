use crate::lifecycle::adapters::codex_plugin_cache::CodexPluginCache;
use crate::lifecycle::adapters::hermes_host_adapter::HermesHostAdapter;
use crate::lifecycle::adapters::mappers::claude_installation_mapper::ClaudeInstallationMapper;
use crate::lifecycle::adapters::mappers::claude_runtime_status_mapper::ClaudeRuntimeStatusMapper;
use crate::lifecycle::adapters::mappers::codex_installation_mapper::CodexInstallationMapper;
use crate::lifecycle::adapters::mappers::codex_runtime_status_mapper::CodexRuntimeStatusMapper;
use crate::lifecycle::adapters::mappers::hermes_runtime_status_mapper::HermesRuntimeStatusMapper;
use crate::lifecycle::adapters::pi_host_adapter::PiHostAdapter;
use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::engine_install_dir::EngineInstallDir;
use crate::lifecycle::domain::hermes_skill_dir::HermesSkillDir;
use crate::lifecycle::domain::host::Host;
use crate::lifecycle::domain::host_installation::HostInstallation;
use crate::lifecycle::domain::host_runtime_status::HostRuntimeStatus;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::marketplace_source::MarketplaceSource;
use crate::lifecycle::domain::pi_agent_home::PiAgentHome;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::host_gateway::HostGateway;
use crate::lifecycle::ports::process_executor::ProcessExecutor;
use crate::lifecycle::ports::process_output::ProcessOutput;

/// Native Claude/Codex/Hermes/Pi adapter. Their JSON and YAML contracts end
/// at the mappers.
pub struct NativeHostGateway<'a> {
    processes: &'a dyn ProcessExecutor,
    codex_cache: CodexPluginCache,
    marketplace: MarketplaceSource,
    hermes: Option<HermesHostAdapter<'a>>,
    pi: Option<PiHostAdapter<'a>>,
}

impl<'a> NativeHostGateway<'a> {
    pub fn new(processes: &'a dyn ProcessExecutor) -> Self {
        Self {
            processes,
            codex_cache: CodexPluginCache::from_environment(),
            marketplace: MarketplaceSource,
            hermes: HermesHostAdapter::new(processes).ok(),
            pi: PiHostAdapter::new(processes).ok(),
        }
    }

    pub fn with_codex_home(
        processes: &'a dyn ProcessExecutor,
        codex_home: impl AsRef<std::path::Path>,
    ) -> Self {
        Self {
            processes,
            codex_cache: CodexPluginCache::new(codex_home),
            marketplace: MarketplaceSource,
            hermes: HermesHostAdapter::new(processes).ok(),
            pi: PiHostAdapter::new(processes).ok(),
        }
    }

    /// A gateway whose Codex and Hermes homes are both explicit, so a test
    /// never mirrors skills into the operator's own Hermes home. It names no
    /// Pi home at all — Pi inventories as absent — so it never reads or
    /// writes the operator's own Pi either; `with_pi_home` adds one.
    pub fn with_homes(
        processes: &'a dyn ProcessExecutor,
        codex_home: impl AsRef<std::path::Path>,
        hermes_home: impl AsRef<std::path::Path>,
    ) -> Result<Self, LifecycleError> {
        let skills = HermesSkillDir::from_home_dir(hermes_home.as_ref())?;
        Ok(Self {
            processes,
            codex_cache: CodexPluginCache::new(codex_home),
            marketplace: MarketplaceSource,
            hermes: Some(HermesHostAdapter::with_skill_dir(processes, skills)),
            pi: None,
        })
    }

    /// Names the Pi agent home explicitly, so a test never mirrors skills
    /// into, or reads the settings of, the operator's own Pi.
    pub fn with_pi_home(
        mut self,
        pi_home: impl Into<std::path::PathBuf>,
    ) -> Result<Self, LifecycleError> {
        let home = PiAgentHome::new(pi_home)?;
        self.pi = Some(PiHostAdapter::with_home(self.processes, home));
        Ok(self)
    }

    fn pi(&self) -> Result<&PiHostAdapter<'a>, LifecycleError> {
        self.pi.as_ref().ok_or_else(|| {
            LifecycleError::HostNotInstalled(
                "no Pi agent directory resolves (PI_CODING_AGENT_DIR or HOME), so the Pi \
                 host cannot be converged"
                    .to_string(),
            )
        })
    }

    fn hermes(&self) -> Result<&HermesHostAdapter<'a>, LifecycleError> {
        self.hermes.as_ref().ok_or_else(|| {
            LifecycleError::HostNotInstalled(
                "no HOME resolves, so the Hermes host cannot be inspected".to_string(),
            )
        })
    }

    fn inventory_host(&self, host: Host) -> Result<Vec<HostInstallation>, LifecycleError> {
        if !self.processes.is_available(host.executable()) {
            return Ok(Vec::new());
        }
        match host {
            Host::Hermes => Ok(self
                .hermes()?
                .installation(&ReleaseVersion::current())?
                .into_iter()
                .collect()),
            // A Pi whose home does not resolve holds no installation this
            // process can see; it must never fail every other host's run.
            Host::Pi => match &self.pi {
                Some(pi) => Ok(pi
                    .installation(&ReleaseVersion::current())?
                    .into_iter()
                    .collect()),
                None => Ok(Vec::new()),
            },
            Host::Claude => {
                let output = self.required(host, &["plugin", "list", "--json"])?;
                ClaudeInstallationMapper::map(output.stdout())
            }
            Host::Codex => {
                let output = self.required(
                    host,
                    &["plugin", "list", "--marketplace", "underpass", "--json"],
                )?;
                CodexInstallationMapper::map_inventory(output.stdout(), &self.codex_cache)
            }
        }
    }

    fn required(&self, host: Host, arguments: &[&str]) -> Result<ProcessOutput, LifecycleError> {
        self.processes
            .execute(host.executable(), arguments)?
            .require_success(host.executable())
            .map_err(|detail| LifecycleError::CommandFailed {
                program: host.executable().to_string(),
                detail,
            })
    }

    fn refresh_claude(&self) -> Result<HostInstallation, LifecycleError> {
        self.required(
            Host::Claude,
            &["plugin", "marketplace", "update", "underpass"],
        )?;
        self.required(
            Host::Claude,
            &["plugin", "update", "kmp@underpass", "--yes"],
        )?;
        self.inventory_host(Host::Claude)?
            .into_iter()
            .find(|installation| installation.participates_in_convergence())
            .ok_or_else(|| {
                LifecycleError::HostNotInstalled(
                    "Claude Code did not report an enabled kmp@underpass after update".to_string(),
                )
            })
    }

    fn refresh_codex(&self) -> Result<HostInstallation, LifecycleError> {
        // Local development marketplaces are intentionally not upgradeable.
        // The add result below is authoritative and still must name the exact
        // requested release before the engine changes.
        let _ = self.processes.execute(
            Host::Codex.executable(),
            &["plugin", "marketplace", "upgrade", "underpass", "--json"],
        );
        let output = self.required(Host::Codex, &["plugin", "add", "kmp@underpass", "--json"])?;
        CodexInstallationMapper::map_add_result(output.stdout())
    }

    /// Hermes holds no versioned artifact of its own: it runs whatever
    /// `kmp-mcp` PATH resolves, and the convergence installs and proves that
    /// engine at the target as its final step. Its release is therefore the
    /// target, never the version of the binary performing the update (#868).
    fn refresh_hermes(&self, target: &ReleaseVersion) -> Result<HostInstallation, LifecycleError> {
        self.hermes()?
            .refresh(target, Self::plugin_root().as_deref())
    }

    fn provision_claude(&self) -> Result<HostInstallation, LifecycleError> {
        let refreshed = self.processes.execute(
            Host::Claude.executable(),
            &["plugin", "marketplace", "update", "underpass"],
        )?;
        if !refreshed.succeeded() {
            let source = self.marketplace.claude_locator();
            self.required(Host::Claude, &["plugin", "marketplace", "add", &source])?;
        }
        self.required(
            Host::Claude,
            &[
                "plugin",
                "install",
                "kmp@underpass",
                "--scope",
                "user",
                "--yes",
            ],
        )?;
        self.inventory_host(Host::Claude)?
            .into_iter()
            .find(HostInstallation::participates_in_convergence)
            .ok_or_else(|| {
                LifecycleError::HostNotInstalled(
                    "Claude Code did not report an enabled kmp@underpass after install".to_string(),
                )
            })
    }

    fn provision_codex(&self) -> Result<HostInstallation, LifecycleError> {
        let refreshed = self.processes.execute(
            Host::Codex.executable(),
            &["plugin", "marketplace", "upgrade", "underpass", "--json"],
        )?;
        if !refreshed.succeeded() {
            let _ = self.processes.execute(
                Host::Codex.executable(),
                &[
                    "plugin",
                    "marketplace",
                    "add",
                    self.marketplace.repository(),
                    "--ref",
                    self.marketplace.distribution_ref(),
                    "--json",
                ],
            );
        }
        let output = self.required(Host::Codex, &["plugin", "add", "kmp@underpass", "--json"])?;
        CodexInstallationMapper::map_add_result(output.stdout())
    }

    fn provision_hermes(
        &self,
        target: &ReleaseVersion,
    ) -> Result<HostInstallation, LifecycleError> {
        self.hermes()?
            .provision(target, Self::plugin_root().as_deref())
    }

    /// The plugin tree skills mirror from. A development checkout carries
    /// one beside this binary; a machine without it converges the MCP
    /// registration alone.
    fn plugin_root() -> Option<std::path::PathBuf> {
        let mut path = std::env::current_exe().ok()?;
        for _ in 0..4 {
            path = path.parent()?.to_path_buf();
        }
        let candidate = path.join("plugins/kmp");
        candidate.is_dir().then_some(candidate)
    }
}

impl HostGateway for NativeHostGateway<'_> {
    fn available_hosts(&self) -> Vec<Host> {
        Host::CONVERGENCE_ORDER
            .into_iter()
            .filter(|host| self.processes.is_available(host.executable()))
            .collect()
    }

    fn inventory(&self) -> Result<Vec<HostInstallation>, LifecycleError> {
        let mut installed = Vec::new();
        for host in Host::CONVERGENCE_ORDER {
            installed.extend(self.inventory_host(host)?);
        }
        Ok(installed)
    }

    fn runtime_status(&self, host: Host) -> Result<HostRuntimeStatus, LifecycleError> {
        if !self.processes.is_available(host.executable()) {
            return Ok(HostRuntimeStatus::Missing);
        }
        match host {
            Host::Claude => {
                let output = self.required(host, &["mcp", "list"])?;
                Ok(ClaudeRuntimeStatusMapper::map(output.stdout()))
            }
            Host::Codex => {
                let output = self.required(host, &["mcp", "list", "--json"])?;
                CodexRuntimeStatusMapper::map(output.stdout())
            }
            Host::Hermes => {
                let output = self.required(host, &["config", "get", "mcp_servers"])?;
                HermesRuntimeStatusMapper::map(output.stdout())
            }
            Host::Pi => Ok(self.pi.as_ref().map_or_else(
                || {
                    HostRuntimeStatus::Failed(
                        "no Pi agent directory resolves, so Pi settings.json cannot be read"
                            .to_string(),
                    )
                },
                PiHostAdapter::runtime_status,
            )),
        }
    }

    fn runtime_engine(
        &self,
        installation: &HostInstallation,
    ) -> Result<EngineExecutable, LifecycleError> {
        match installation.host() {
            Host::Claude => {
                let directory = EngineInstallDir::new(installation.root().engine_dir())?;
                Ok(EngineExecutable::installed_at(directory.executable()))
            }
            Host::Codex => self
                .processes
                .resolve("kmp-mcp")
                .map(EngineExecutable::installed_at)
                .ok_or_else(|| {
                    LifecycleError::HostNotInstalled(
                        "Codex declares `kmp-mcp`, but no executable resolves on PATH".to_string(),
                    )
                }),
            Host::Hermes => self.hermes()?.runtime_engine(),
            Host::Pi => self.pi()?.runtime_engine(),
        }
    }

    fn provision(
        &self,
        host: Host,
        target: &ReleaseVersion,
    ) -> Result<HostInstallation, LifecycleError> {
        if !self.processes.is_available(host.executable()) {
            return Err(LifecycleError::HostNotInstalled(format!(
                "{} was selected, but `{}` is not on PATH",
                host,
                host.executable()
            )));
        }
        let installation = match host {
            Host::Claude => self.provision_claude()?,
            Host::Codex => self.provision_codex()?,
            Host::Hermes => self.provision_hermes(target)?,
            Host::Pi => self
                .pi()?
                .provision(target, Self::plugin_root().as_deref())?,
        };
        installation.require_release(target)?;
        Ok(installation)
    }

    fn refresh(
        &self,
        host: Host,
        target: &ReleaseVersion,
    ) -> Result<HostInstallation, LifecycleError> {
        if !self.processes.is_available(host.executable()) {
            return Err(LifecycleError::HostNotInstalled(format!(
                "{} was selected, but `{}` is not on PATH",
                host,
                host.executable()
            )));
        }
        let installation = match host {
            Host::Claude => self.refresh_claude()?,
            Host::Codex => self.refresh_codex()?,
            Host::Hermes => self.refresh_hermes(target)?,
            Host::Pi => self.pi()?.refresh(target, Self::plugin_root().as_deref())?,
        };
        installation.require_release(target)?;
        Ok(installation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::adapters::tests_support::FakeProcessExecutor;

    fn without_pi_home(processes: &FakeProcessExecutor) -> NativeHostGateway<'_> {
        let homes = std::env::temp_dir();
        NativeHostGateway::with_homes(processes, homes.join("codex"), homes.join("hermes"))
            .expect("gateway")
    }

    #[test]
    fn an_unresolvable_pi_home_inventories_as_no_installation() {
        let processes = FakeProcessExecutor::expecting(vec![]);
        let gateway = without_pi_home(&processes);

        assert!(
            gateway
                .inventory_host(Host::Pi)
                .expect("inventory never fails for Pi")
                .is_empty()
        );
        assert!(matches!(
            gateway.runtime_status(Host::Pi).expect("status"),
            HostRuntimeStatus::Failed(_)
        ));
    }

    #[test]
    fn an_explicit_pi_without_a_home_still_fails_to_converge() {
        let processes = FakeProcessExecutor::expecting(vec![]);
        let gateway = without_pi_home(&processes);

        let error = gateway
            .provision(Host::Pi, &ReleaseVersion::current())
            .expect_err("no Pi home");
        assert!(error.to_string().contains("PI_CODING_AGENT_DIR"), "{error}");
    }
}
