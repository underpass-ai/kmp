use std::path::PathBuf;
use std::sync::Mutex;

use kmp_mcp::lifecycle::domain::engine_executable::EngineExecutable;
use kmp_mcp::lifecycle::domain::host::Host;
use kmp_mcp::lifecycle::domain::host_installation::HostInstallation;
use kmp_mcp::lifecycle::domain::host_runtime_status::HostRuntimeStatus;
use kmp_mcp::lifecycle::domain::lifecycle_error::LifecycleError;
use kmp_mcp::lifecycle::domain::release_version::ReleaseVersion;
use kmp_mcp::lifecycle::ports::host_gateway::HostGateway;

pub struct FakeHostGateway {
    installed: Vec<HostInstallation>,
    provisions: Mutex<Vec<Host>>,
    refreshes: Mutex<Vec<Host>>,
    refreshed_version: Option<ReleaseVersion>,
}

impl FakeHostGateway {
    pub fn with_installations(installed: Vec<HostInstallation>) -> Self {
        Self {
            installed,
            provisions: Mutex::new(Vec::new()),
            refreshes: Mutex::new(Vec::new()),
            refreshed_version: None,
        }
    }

    pub fn returning_version(mut self, version: ReleaseVersion) -> Self {
        self.refreshed_version = Some(version);
        self
    }

    pub fn refreshes(&self) -> Vec<Host> {
        self.refreshes.lock().expect("refresh lock").clone()
    }

    pub fn provisions(&self) -> Vec<Host> {
        self.provisions.lock().expect("provision lock").clone()
    }

    fn installation_for(&self, host: Host, target: &ReleaseVersion) -> HostInstallation {
        let root = match host {
            Host::Claude => "/tmp/claude",
            Host::Codex => "/tmp/codex",
        };
        HostInstallation::discovered(
            host,
            self.refreshed_version.as_ref().unwrap_or(target).clone(),
            kmp_mcp::lifecycle::PluginRoot::new(root).expect("fake plugin root"),
            true,
        )
    }
}

impl HostGateway for FakeHostGateway {
    fn available_hosts(&self) -> Vec<Host> {
        Host::CONVERGENCE_ORDER.to_vec()
    }

    fn inventory(&self) -> Result<Vec<HostInstallation>, LifecycleError> {
        Ok(self.installed.clone())
    }

    fn runtime_status(&self, host: Host) -> Result<HostRuntimeStatus, LifecycleError> {
        Ok(match host {
            Host::Claude => HostRuntimeStatus::Connected,
            Host::Codex => HostRuntimeStatus::Registered,
        })
    }

    fn runtime_engine(
        &self,
        installation: &HostInstallation,
    ) -> Result<EngineExecutable, LifecycleError> {
        let path = if installation.host().owns_plugin_engine() {
            installation.root().engine_dir().join("kmp-mcp")
        } else {
            PathBuf::from("/tmp/shared/kmp-mcp")
        };
        Ok(EngineExecutable::installed_at(path))
    }

    fn provision(
        &self,
        host: Host,
        target: &ReleaseVersion,
    ) -> Result<HostInstallation, LifecycleError> {
        self.provisions.lock().expect("provision lock").push(host);
        Ok(self.installation_for(host, target))
    }

    fn refresh(
        &self,
        host: Host,
        target: &ReleaseVersion,
    ) -> Result<HostInstallation, LifecycleError> {
        self.refreshes.lock().expect("refresh lock").push(host);
        let existing = self
            .installed
            .iter()
            .find(|installation| installation.host() == host)
            .ok_or_else(|| LifecycleError::HostNotInstalled(host.to_string()))?;
        Ok(HostInstallation::discovered(
            host,
            self.refreshed_version.as_ref().unwrap_or(target).clone(),
            existing.root().clone(),
            true,
        ))
    }
}
