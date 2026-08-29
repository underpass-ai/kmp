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
        let Some(kmp) = servers.iter().find(|server| server["name"] == "kmp") else {
            return Ok(HostRuntimeStatus::Missing);
        };
        Ok(if kmp["enabled"].as_bool().unwrap_or(false) {
            HostRuntimeStatus::Registered
        } else {
            HostRuntimeStatus::Disabled
        })
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
}
