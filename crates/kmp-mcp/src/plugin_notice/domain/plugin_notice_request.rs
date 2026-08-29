use crate::lifecycle::domain::plugin_root::PluginRoot;

/// Validated input to the notice use case.
pub struct PluginNoticeRequest {
    plugin_root: PluginRoot,
}

impl PluginNoticeRequest {
    pub fn new(plugin_root: PluginRoot) -> Self {
        Self { plugin_root }
    }

    pub fn plugin_root(&self) -> &PluginRoot {
        &self.plugin_root
    }
}
