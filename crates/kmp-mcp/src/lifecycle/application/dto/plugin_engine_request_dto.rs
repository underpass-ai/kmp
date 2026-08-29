use crate::lifecycle::domain::plugin_engine_candidate::PluginEngineCandidate;
use crate::lifecycle::domain::plugin_root::PluginRoot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginEngineRequestDto {
    pub plugin_root: PluginRoot,
    pub candidates: Vec<PluginEngineCandidate>,
}
