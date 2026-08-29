use crate::lifecycle::application::dto::plugin_engine_resolution_dto::PluginEngineResolutionDto;
use crate::lifecycle::domain::plugin_engine_resolution::PluginEngineResolution;

pub struct PluginEngineResolutionMapper;

impl PluginEngineResolutionMapper {
    pub fn to_dto(resolution: &PluginEngineResolution) -> PluginEngineResolutionDto {
        PluginEngineResolutionDto {
            executable: resolution.selected().as_path().display().to_string(),
            warning: resolution.warning().map(str::to_string),
            version: resolution.version().to_string(),
        }
    }
}
