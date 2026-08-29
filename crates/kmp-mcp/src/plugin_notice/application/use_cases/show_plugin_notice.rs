use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::plugin_notice::domain::plugin_notice::PluginNotice;
use crate::plugin_notice::domain::plugin_notice_error::PluginNoticeError;
use crate::plugin_notice::domain::plugin_notice_request::PluginNoticeRequest;
use crate::plugin_notice::ports::latest_release_source::LatestReleaseSource;
use crate::plugin_notice::ports::plugin_manifest_repository::PluginManifestRepository;

/// Decides whether the session-start hook has anything actionable to say.
pub struct ShowPluginNotice<'a> {
    manifests: &'a dyn PluginManifestRepository,
    releases: &'a dyn LatestReleaseSource,
}

impl<'a> ShowPluginNotice<'a> {
    pub fn new(
        manifests: &'a dyn PluginManifestRepository,
        releases: &'a dyn LatestReleaseSource,
    ) -> Self {
        Self {
            manifests,
            releases,
        }
    }

    pub fn execute(
        &self,
        request: &PluginNoticeRequest,
    ) -> Result<PluginNotice, PluginNoticeError> {
        let plugin = self.manifests.version(request.plugin_root())?;
        let engine = ReleaseVersion::current();
        if !plugin.represents_same_release(&engine) {
            return Ok(PluginNotice::misaligned(engine, plugin));
        }
        Ok(PluginNotice::from_latest(plugin, self.releases.latest()))
    }
}
