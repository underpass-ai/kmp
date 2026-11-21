use serde_json::Value;

use crate::lifecycle::domain::host_runtime_status::HostRuntimeStatus;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;

/// Anti-corruption mapper for Codex's effective MCP inventory DTO.
#[derive(Clone, Copy, Debug, Default)]
pub struct CodexRuntimeStatusMapper;

impl CodexRuntimeStatusMapper {
    pub fn map(json: &str) -> Result<HostRuntimeStatus, LifecycleError> {
        let body: Value = serde_json::from_str(json).map_err(|error| {
            LifecycleError::InvalidHostResponse(format!(
                "Codex returned invalid MCP inventory JSON: {error}"
            ))
        })?;
        let servers = body.as_array().ok_or_else(|| {
            LifecycleError::InvalidHostResponse("Codex MCP inventory is not an array".to_string())
        })?;
        // Every entry named `kmp` counts, not the first one listed: a second
        // owner may sit ahead of the plugin's own, and either being enabled
        // is a registration that serves the tools.
        let mut named_kmp = servers
            .iter()
            .filter(|server| server["name"] == "kmp")
            .peekable();
        if named_kmp.peek().is_none() {
            return Ok(HostRuntimeStatus::Missing);
        }
        Ok(
            if named_kmp.any(|server| server["enabled"].as_bool().unwrap_or(false)) {
                HostRuntimeStatus::Registered
            } else {
                HostRuntimeStatus::Disabled
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_the_effective_codex_registration() {
        let status = CodexRuntimeStatusMapper::map(
            r#"[{"name":"kmp","enabled":true,"transport":{"command":"kmp-mcp"}}]"#,
        )
        .expect("runtime status");
        assert_eq!(status, HostRuntimeStatus::Registered);
    }

    #[test]
    fn a_disabled_entry_listed_first_does_not_hide_an_enabled_one() {
        let status = CodexRuntimeStatusMapper::map(
            r#"[{"name":"kmp","enabled":false},{"name":"kmp","enabled":true}]"#,
        )
        .expect("runtime status");
        assert_eq!(status, HostRuntimeStatus::Registered);
        let disabled =
            CodexRuntimeStatusMapper::map(r#"[{"name":"other","enabled":true},{"name":"kmp"}]"#)
                .expect("runtime status");
        assert_eq!(disabled, HostRuntimeStatus::Disabled);
        let missing = CodexRuntimeStatusMapper::map(r#"[]"#).expect("runtime status");
        assert_eq!(missing, HostRuntimeStatus::Missing);
    }
}
