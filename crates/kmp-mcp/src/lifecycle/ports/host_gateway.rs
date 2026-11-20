use crate::lifecycle::domain::host::Host;
use crate::lifecycle::domain::host_installation::HostInstallation;
use crate::lifecycle::domain::host_inventory::HostInventory;
use crate::lifecycle::domain::host_runtime_status::HostRuntimeStatus;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::release_version::ReleaseVersion;

/// Outbound port for plugin-manager inventory and mutation.
pub trait HostGateway: Send + Sync {
    fn available_hosts(&self) -> Vec<Host>;

    /// Every host's installation, host by host: one host whose inventory
    /// cannot be read is named in the result and never hides the others.
    fn inventory(&self) -> HostInventory;

    /// Whether what `installation` claims is really on disk: a plugin
    /// manager's manifest inside its root, or every skill a host without one
    /// was given. A registry entry or a registration alone is not an
    /// installation (S12, S13).
    fn verify_on_disk(
        &self,
        installation: &HostInstallation,
        target: &ReleaseVersion,
    ) -> Result<(), LifecycleError>;

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
