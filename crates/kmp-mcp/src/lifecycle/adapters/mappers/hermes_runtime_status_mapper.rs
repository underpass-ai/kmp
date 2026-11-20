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
    /// Parse Hermes config YAML into a dynamic value.
    ///
    /// The options keep the YAML 1.2 core reading Hermes output has always
    /// had here: only `true`/`false` are booleans, numbers with leading zeros
    /// or underscores stay strings, `<<` is an ordinary key and a repeated
    /// key keeps its last value.
    pub fn parse(config_yaml: &str) -> Result<Value, serde_saphyr::Error> {
        serde_saphyr::from_str_with_options(
            config_yaml,
            serde_saphyr::options! {
                strict_booleans: true,
                leading_zero_numbers_as_strings: true,
                underscored_numbers_as_strings: true,
                duplicate_keys: serde_saphyr::DuplicateKeyPolicy::PassThrough,
                merge_keys: serde_saphyr::MergeKeyPolicy::AsOrdinary,
            },
        )
    }

    pub fn map(config_yaml: &str) -> Result<HostRuntimeStatus, LifecycleError> {
        // A Hermes with no MCP server configured prints nothing (or a YAML
        // null): KMP is simply not registered there. That is an answer, not
        // a broken host, and reading it as one stopped every host (S11).
        if config_yaml.trim().is_empty() {
            return Ok(HostRuntimeStatus::Missing);
        }
        let body = Self::parse(config_yaml).map_err(|error| {
            LifecycleError::InvalidHostResponse(format!(
                "Hermes returned invalid MCP config YAML: {error}"
            ))
        })?;
        if body.is_null() {
            return Ok(HostRuntimeStatus::Missing);
        }
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
    fn a_hermes_with_no_servers_configured_has_kmp_missing_not_broken() {
        for empty in ["", "\n", "null\n", "~\n"] {
            assert_eq!(
                HermesRuntimeStatusMapper::map(empty).expect("an answer"),
                HostRuntimeStatus::Missing,
                "{empty:?}"
            );
        }
    }

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

    #[test]
    fn a_scalar_is_still_not_a_mapping_of_servers() {
        let error = HermesRuntimeStatusMapper::map("no servers\n").expect_err("scalar output");
        assert!(error.to_string().contains("not a mapping"));
    }

    #[test]
    fn only_yaml_core_spellings_are_booleans() {
        // `no` is a string under the YAML 1.2 core schema, so it does not
        // switch the registration off, exactly as before the parser swap.
        let status = HermesRuntimeStatusMapper::map("kmp:\n  command: kmp-mcp\n  enabled: no\n")
            .expect("status");
        assert_eq!(status, HostRuntimeStatus::Registered);
        let body = HermesRuntimeStatusMapper::parse("a: yes\nb: 0755\nc: 1_000\n").expect("yaml");
        assert_eq!(body["a"], "yes");
        assert_eq!(body["b"], "0755");
        assert_eq!(body["c"], "1_000");
    }

    #[test]
    fn a_repeated_key_keeps_its_last_value() {
        let status = HermesRuntimeStatusMapper::map(
            "kmp:\n  command: kmp-mcp\nkmp:\n  command: kmp-mcp\n  enabled: false\n",
        )
        .expect("status");
        assert_eq!(status, HostRuntimeStatus::Disabled);
    }
}
