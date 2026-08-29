use crate::lifecycle::domain::host::Host;
use crate::lifecycle::domain::host_installation::HostInstallation;
use crate::lifecycle::domain::host_runtime_status::HostRuntimeStatus;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::release_version::ReleaseVersion;

/// Outbound port for plugin-manager inventory and mutation.
pub trait HostGateway: Send + Sync {
    fn available_hosts(&self) -> Vec<Host>;

    fn inventory(&self) -> Result<Vec<HostInstallation>, LifecycleError>;

    fn runtime_status(&self, host: Host) -> Result<HostRuntimeStatus, LifecycleError>;

    fn runtime_engine(
        &self,
        installation: &HostInstallation,
    ) -> Result<EngineExecutable, LifecycleError>;

    fn provision(
        &self,
        host: Host,
        target: &ReleaseVersion,
    ) -> Result<HostInstallation, LifecycleError>;

    fn refresh(
        &self,
        host: Host,
        target: &ReleaseVersion,
    ) -> Result<HostInstallation, LifecycleError>;
}
use crate::lifecycle::domain::engine_executable::EngineExecutable;
