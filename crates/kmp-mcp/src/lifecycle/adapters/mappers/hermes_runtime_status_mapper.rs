use serde_json::Value;

use crate::lifecycle::domain::host_runtime_status::HostRuntimeStatus;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;

/// Anti-corruption mapper for `hermes config get mcp_servers`.
///
/// Hermes has no `mcp list --json`; the authoritative machine-readable
/// evidence is its own config surface. Each key is one server name carrying
/// `command` (or `url`) plus `enabled`.
#[derive(Clone, Copy, Debug, Default)]
pub struct HermesRuntimeStatusMapper;

impl HermesRuntimeStatusMapper {
    pub fn map(config_yaml: &str) -> Result<HostRuntimeStatus, LifecycleError> {
        let body: Value = serde_yaml::from_str(config_yaml).map_err(|error| {
            LifecycleError::InvalidHostResponse(format!(
                "Hermes returned invalid MCP config YAML: {error}"
            ))
        })?;
        let servers = body.as_object().ok_or_else(|| {
            LifecycleError::InvalidHostResponse(
                "Hermes MCP config is not a mapping of server names".to_string(),
            )
        })?;
        let Some(kmp) = servers.get("kmp") else {
            return Ok(HostRuntimeStatus::Missing);
        };
        let enabled = kmp["enabled"].as_bool().unwrap_or(true);
        let command = kmp["command"].as_str().or_else(|| kmp["url"].as_str());
        match (enabled, command) {
            (false, _) => Ok(HostRuntimeStatus::Disabled),
            (true, Some(_)) => Ok(HostRuntimeStatus::Registered),
            (true, None) => Ok(HostRuntimeStatus::Failed(
                "the KMP entry in Hermes MCP config names no command or url".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registered_enabled_server_is_a_usable_registration() {
        let status = HermesRuntimeStatusMapper::map(
            "kmp:\n  command: /opt/kmp/run-embedded-mcp.sh\n  enabled: true\n",
        )
        .expect("status");
        assert_eq!(status, HostRuntimeStatus::Registered);
    }

    #[test]
    fn a_disabled_server_is_disabled_even_though_it_names_a_command() {
        let status = HermesRuntimeStatusMapper::map(
            "kmp:\n  command: /opt/kmp/run-embedded-mcp.sh\n  enabled: false\n",
        )
        .expect("status");
        assert_eq!(status, HostRuntimeStatus::Disabled);
    }

    #[test]
    fn other_servers_do_not_satisfy_the_kmp_registration() {
        let status = HermesRuntimeStatusMapper::map(
            "time:\n  command: uvx\n  args: [mcp-server-time]\n  enabled: true\n",
        )
        .expect("status");
        assert_eq!(status, HostRuntimeStatus::Missing);
    }

    #[test]
    fn an_enabled_entry_without_transport_is_a_failure_with_a_name() {
        let status = HermesRuntimeStatusMapper::map("kmp:\n  enabled: true\n").expect("status");
        assert!(matches!(status, HostRuntimeStatus::Failed(_)));
    }

    #[test]
    fn http_registrations_register_the_same_way() {
        let status = HermesRuntimeStatusMapper::map(
            "kmp:\n  url: https://kmp.internal/mcp\n  enabled: true\n",
        )
        .expect("status");
        assert_eq!(status, HostRuntimeStatus::Registered);
    }
}
