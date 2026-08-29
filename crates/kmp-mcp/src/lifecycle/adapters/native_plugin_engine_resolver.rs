use serde_json::Value;

use crate::lifecycle::adapters::native_plugin_engine_probe::NativePluginEngineProbe;
use crate::lifecycle::adapters::plugin_engine_cli_parser::PluginEngineCliParser;
use crate::lifecycle::application::dto::plugin_engine_resolution_dto::PluginEngineResolutionDto;
use crate::lifecycle::application::mappers::plugin_engine_resolution_mapper::PluginEngineResolutionMapper;
use crate::lifecycle::application::use_cases::resolve_plugin_engine::ResolvePluginEngine;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::release_version::ReleaseVersion;

pub struct NativePluginEngineResolver;

impl NativePluginEngineResolver {
    pub fn execute(arguments: &[&str]) -> Result<PluginEngineResolutionDto, LifecycleError> {
        let request = PluginEngineCliParser::parse(arguments)?;
        let mut expected: Option<ReleaseVersion> = None;
        for relative in [".codex-plugin/plugin.json", ".claude-plugin/plugin.json"] {
            let path = request.plugin_root.as_path().join(relative);
            let text = std::fs::read_to_string(&path).map_err(|error| LifecycleError::Io {
                path: path.clone(),
                detail: error.to_string(),
            })?;
            let body: Value = serde_json::from_str(&text).map_err(|error| {
                LifecycleError::InvalidHostResponse(format!(
                    "{} is invalid: {error}",
                    path.display()
                ))
            })?;
            let version = body["version"].as_str().ok_or_else(|| {
                LifecycleError::InvalidHostResponse(format!("{} has no version", path.display()))
            })?;
            let version = ReleaseVersion::parse(version)?;
            if let Some(previous) = expected.as_ref()
                && !previous.represents_same_release(&version)
            {
                return Err(LifecycleError::HostVersionMismatch(format!(
                    "plugin manifests disagree: {previous} != {version}"
                )));
            }
            expected = Some(version);
        }
        let expected = expected.ok_or_else(|| {
            LifecycleError::InvalidHostResponse("plugin has no manifests".to_string())
        })?;
        let resolution = ResolvePluginEngine::new(&NativePluginEngineProbe)
            .execute(&expected, &request.candidates)?;
        Ok(PluginEngineResolutionMapper::to_dto(&resolution))
    }
}
