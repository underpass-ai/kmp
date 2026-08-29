use serde_json::Value;

use crate::lifecycle::adapters::codex_plugin_cache::CodexPluginCache;
use crate::lifecycle::domain::host::Host;
use crate::lifecycle::domain::host_installation::HostInstallation;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::lifecycle::domain::release_version::ReleaseVersion;

/// Anti-corruption mapper for Codex's plugin-list and plugin-add DTOs.
#[derive(Clone, Copy, Debug, Default)]
pub struct CodexInstallationMapper;

impl CodexInstallationMapper {
    pub fn map_inventory(
        json: &str,
        cache: &CodexPluginCache,
    ) -> Result<Vec<HostInstallation>, LifecycleError> {
        let body: Value = serde_json::from_str(json).map_err(|error| {
            LifecycleError::InvalidHostResponse(format!(
                "Codex returned invalid plugin JSON: {error}"
            ))
        })?;
        let installed = body["installed"].as_array().ok_or_else(|| {
            LifecycleError::InvalidHostResponse(
                "Codex plugin inventory omitted installed[]".to_string(),
            )
        })?;
        installed
            .iter()
            .filter(|plugin| plugin["pluginId"] == "kmp@underpass")
            .map(|plugin| Self::map_plugin(plugin, cache))
            .collect()
    }

    pub fn map_add_result(json: &str) -> Result<HostInstallation, LifecycleError> {
        let plugin: Value = serde_json::from_str(json).map_err(|error| {
            LifecycleError::InvalidHostResponse(format!(
                "Codex returned invalid plugin-add JSON: {error}"
            ))
        })?;
        let version = plugin["version"].as_str().ok_or_else(|| {
            LifecycleError::InvalidHostResponse(
                "Codex plugin-add result omitted version".to_string(),
            )
        })?;
        let root = plugin["installedPath"].as_str().ok_or_else(|| {
            LifecycleError::InvalidHostResponse(
                "Codex plugin-add result omitted installedPath".to_string(),
            )
        })?;
        Ok(HostInstallation::discovered(
            Host::Codex,
            ReleaseVersion::parse(version)?,
            PluginRoot::new(root)?,
            true,
        ))
    }

    fn map_plugin(
        plugin: &Value,
        cache: &CodexPluginCache,
    ) -> Result<HostInstallation, LifecycleError> {
        let version = plugin["version"].as_str().ok_or_else(|| {
            LifecycleError::InvalidHostResponse("Codex KMP inventory omitted version".to_string())
        })?;
        let version = ReleaseVersion::parse(version)?;
        let marketplace = plugin["marketplaceName"].as_str().unwrap_or("underpass");
        let name = plugin["name"].as_str().unwrap_or("kmp");
        Ok(HostInstallation::discovered(
            Host::Codex,
            version.clone(),
            cache.plugin_root(marketplace, name, &version)?,
            plugin["enabled"].as_bool().unwrap_or(true),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_marketplace_source_never_overrides_the_installed_cache_root() {
        let installations = CodexInstallationMapper::map_inventory(
            r#"{
                "installed": [{
                    "pluginId": "kmp@underpass",
                    "name": "kmp",
                    "marketplaceName": "underpass",
                    "version": "0.5.1",
                    "installed": true,
                    "enabled": true,
                    "source": {
                        "source": "local",
                        "path": "/tmp/codex/.tmp/marketplaces/underpass/plugins/kmp"
                    },
                    "marketplaceSource": {
                        "sourceType": "git",
                        "source": "https://github.com/underpass-ai/kmp.git"
                    }
                }],
                "available": []
            }"#,
            &CodexPluginCache::new("/tmp/codex"),
        )
        .expect("inventory");
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].host(), Host::Codex);
        assert_eq!(
            installations[0].root().as_path(),
            std::path::Path::new("/tmp/codex/plugins/cache/underpass/kmp/0.5.1")
        );
    }
}
