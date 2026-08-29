use crate::lifecycle::adapters::codex_plugin_cache::CodexPluginCache;
use crate::lifecycle::adapters::mappers::claude_installation_mapper::ClaudeInstallationMapper;
use crate::lifecycle::adapters::mappers::claude_runtime_status_mapper::ClaudeRuntimeStatusMapper;
use crate::lifecycle::adapters::mappers::codex_installation_mapper::CodexInstallationMapper;
use crate::lifecycle::adapters::mappers::codex_runtime_status_mapper::CodexRuntimeStatusMapper;
use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::engine_install_dir::EngineInstallDir;
use crate::lifecycle::domain::host::Host;
use crate::lifecycle::domain::host_installation::HostInstallation;
use crate::lifecycle::domain::host_runtime_status::HostRuntimeStatus;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::marketplace_source::MarketplaceSource;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::host_gateway::HostGateway;
use crate::lifecycle::ports::process_executor::ProcessExecutor;
use crate::lifecycle::ports::process_output::ProcessOutput;

/// Native Claude/Codex adapter. Their JSON contracts end at the mappers.
pub struct NativeHostGateway<'a> {
    processes: &'a dyn ProcessExecutor,
    codex_cache: CodexPluginCache,
    marketplace: MarketplaceSource,
}

impl<'a> NativeHostGateway<'a> {
    pub fn new(processes: &'a dyn ProcessExecutor) -> Self {
        Self {
            processes,
            codex_cache: CodexPluginCache::from_environment(),
            marketplace: MarketplaceSource,
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
        }
    }

    fn inventory_host(&self, host: Host) -> Result<Vec<HostInstallation>, LifecycleError> {
        if !self.processes.is_available(host.executable()) {
            return Ok(Vec::new());
        }
        let output = match host {
            Host::Claude => self.required(host, &["plugin", "list", "--json"]),
            Host::Codex => self.required(
                host,
                &["plugin", "list", "--marketplace", "underpass", "--json"],
            ),
        }?;
        match host {
            Host::Claude => ClaudeInstallationMapper::map(output.stdout()),
            Host::Codex => {
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
        };
        installation.require_release(target)?;
        Ok(installation)
    }
}
