use std::process::Command;

use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::plugin_engine_probe::PluginEngineProbe;

pub struct NativePluginEngineProbe;

impl PluginEngineProbe for NativePluginEngineProbe {
    fn version(
        &self,
        executable: &EngineExecutable,
    ) -> Result<Option<ReleaseVersion>, LifecycleError> {
        if !executable.as_path().is_file() {
            return Ok(None);
        }
        let output = Command::new(executable.as_path())
            .arg("--version")
            .output()
            .map_err(|error| LifecycleError::Io {
                path: executable.as_path().to_path_buf(),
                detail: error.to_string(),
            })?;
        if !output.status.success() {
            return Ok(None);
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut fields = text.split_whitespace();
        if fields.next() != Some("kmp-mcp") {
            return Ok(None);
        }
        fields.next().map(ReleaseVersion::parse).transpose()
    }
}
