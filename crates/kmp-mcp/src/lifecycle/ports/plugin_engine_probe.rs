use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::release_version::ReleaseVersion;

pub trait PluginEngineProbe {
    fn version(
        &self,
        executable: &EngineExecutable,
    ) -> Result<Option<ReleaseVersion>, LifecycleError>;
}
