use std::fs;

use serde_json::Value;

use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::plugin_notice::domain::plugin_notice_error::PluginNoticeError;
use crate::plugin_notice::ports::plugin_manifest_repository::PluginManifestRepository;

#[derive(Clone, Copy, Debug, Default)]
pub struct JsonPluginManifestRepository;

impl PluginManifestRepository for JsonPluginManifestRepository {
    fn version(&self, root: &PluginRoot) -> Result<ReleaseVersion, PluginNoticeError> {
        let path = root.as_path().join(".codex-plugin/plugin.json");
        let bytes = fs::read(&path).map_err(|error| {
            PluginNoticeError::InvalidManifest(format!(
                "could not read `{}`: {error}",
                path.display()
            ))
        })?;
        let manifest: Value = serde_json::from_slice(&bytes).map_err(|error| {
            PluginNoticeError::InvalidManifest(format!("`{}` is not JSON: {error}", path.display()))
        })?;
        let version = manifest["version"].as_str().ok_or_else(|| {
            PluginNoticeError::InvalidManifest(format!("`{}` omitted version", path.display()))
        })?;
        ReleaseVersion::parse(version)
            .map_err(|error| PluginNoticeError::InvalidManifest(error.to_string()))
    }
}
