use crate::guide::domain::guide_plugin_root::GuidePluginRoot;
use crate::guide::domain::guide_sync_mode::GuideSyncMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideSyncRequestDto {
    plugin_root: GuidePluginRoot,
    mode: GuideSyncMode,
}

impl GuideSyncRequestDto {
    pub fn new(plugin_root: GuidePluginRoot, mode: GuideSyncMode) -> Self {
        Self { plugin_root, mode }
    }

    pub fn plugin_root(&self) -> &GuidePluginRoot {
        &self.plugin_root
    }

    pub fn mode(&self) -> GuideSyncMode {
        self.mode
    }
}
