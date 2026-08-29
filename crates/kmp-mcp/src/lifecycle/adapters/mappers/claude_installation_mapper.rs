use serde_json::Value;

use crate::lifecycle::domain::host::Host;
use crate::lifecycle::domain::host_installation::HostInstallation;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::lifecycle::domain::release_version::ReleaseVersion;

/// Anti-corruption mapper for Claude Code's plugin-list DTO.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClaudeInstallationMapper;

impl ClaudeInstallationMapper {
    pub fn map(json: &str) -> Result<Vec<HostInstallation>, LifecycleError> {
        let body: Value = serde_json::from_str(json).map_err(|error| {
            LifecycleError::InvalidHostResponse(format!(
                "Claude Code returned invalid plugin JSON: {error}"
            ))
        })?;
        let plugins = body.as_array().ok_or_else(|| {
            LifecycleError::InvalidHostResponse(
                "Claude Code plugin inventory is not an array".to_string(),
            )
        })?;
        plugins
            .iter()
            .filter(|plugin| plugin["id"] == "kmp@underpass")
            .map(|plugin| {
                let version = plugin["version"].as_str().ok_or_else(|| {
                    LifecycleError::InvalidHostResponse(
                        "Claude KMP inventory omitted version".to_string(),
                    )
                })?;
                let root = plugin["installPath"].as_str().ok_or_else(|| {
                    LifecycleError::InvalidHostResponse(
                        "Claude KMP inventory omitted installPath".to_string(),
                    )
                })?;
                Ok(HostInstallation::discovered(
                    Host::Claude,
                    ReleaseVersion::parse(version)?,
                    PluginRoot::new(root)?,
                    plugin["enabled"].as_bool().unwrap_or(true),
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_dto_does_not_leak_into_the_domain() {
        let installations = ClaudeInstallationMapper::map(
            r#"[{"id":"kmp@underpass","version":"0.4.2","enabled":true,"installPath":"/tmp/claude/kmp"}]"#,
        )
        .expect("inventory");
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].host(), Host::Claude);
    }
}
