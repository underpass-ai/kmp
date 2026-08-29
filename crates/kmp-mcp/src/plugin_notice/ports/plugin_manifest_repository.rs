use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::plugin_notice::domain::plugin_notice_error::PluginNoticeError;

pub trait PluginManifestRepository {
    fn version(&self, root: &PluginRoot) -> Result<ReleaseVersion, PluginNoticeError>;
}
