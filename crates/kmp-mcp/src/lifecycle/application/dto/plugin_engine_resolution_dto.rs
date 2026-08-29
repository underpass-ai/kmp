#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginEngineResolutionDto {
    pub executable: String,
    pub warning: Option<String>,
    pub version: String,
}
