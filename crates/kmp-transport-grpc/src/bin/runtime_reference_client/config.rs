use std::env;
use std::io;

use kmp_transport_grpc::agentic_reference::{AgentRequest, SUMMARY_PATH};

#[derive(Debug)]
pub(crate) struct AppConfig {
    pub(crate) kernel_grpc_endpoint: String,
    pub(crate) runtime_base_url: String,
    pub(crate) request: AgentRequest,
}

impl AppConfig {
    pub(crate) fn from_env() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    fn from_lookup<F>(lookup: F) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
    where
        F: Fn(&str) -> Option<String>,
    {
        let kernel_grpc_endpoint = required_value(&lookup, "KERNEL_GRPC_ENDPOINT")?;
        let runtime_base_url = required_value(&lookup, "RUNTIME_BASE_URL")?;
        let root_node_id = required_value(&lookup, "ROOT_NODE_ID")?;
        let root_node_kind = lookup_or_default(&lookup, "ROOT_NODE_KIND", "workspace");
        let role = lookup_or_default(&lookup, "AGENT_ROLE", "implementer");
        let focus_node_kind = lookup_or_default(&lookup, "AGENT_FOCUS_NODE_KIND", "work_item");
        let requested_scopes = parse_scopes(lookup_or_default(
            &lookup,
            "AGENT_SCOPES",
            "implementation,dependencies",
        ));
        let token_budget = parse_u32_value(&lookup, "AGENT_TOKEN_BUDGET", 1200)?;
        let summary_path = lookup_or_default(&lookup, "AGENT_SUMMARY_PATH", SUMMARY_PATH);

        Ok(Self {
            kernel_grpc_endpoint,
            runtime_base_url,
            request: AgentRequest {
                root_node_id,
                root_node_kind,
                role,
                focus_node_kind,
                requested_scopes,
                token_budget,
                summary_path,
            },
        })
    }
}

fn required_value<F>(lookup: &F, key: &str) -> io::Result<String>
where
    F: Fn(&str) -> Option<String>,
{
    lookup(key)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("missing env `{key}`")))
}

fn lookup_or_default<F>(lookup: &F, key: &str, default_value: &str) -> String
where
    F: Fn(&str) -> Option<String>,
{
    lookup(key).unwrap_or_else(|| default_value.to_string())
}

fn parse_u32_value<F>(lookup: &F, key: &str, default_value: u32) -> io::Result<u32>
where
    F: Fn(&str) -> Option<String>,
{
    lookup(key)
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value.parse::<u32>().map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("invalid `{key}` value `{value}`: {error}"),
                )
            })
        })
        .transpose()
        .map(|value| value.unwrap_or(default_value))
}

fn parse_scopes(value: String) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::AppConfig;

    #[test]
    fn from_env_uses_defaults_for_optional_values() {
        let env = required_env();

        let config = AppConfig::from_lookup(|key| env.get(key).cloned())
            .expect("config should load from defaults");

        assert_eq!(config.kernel_grpc_endpoint, "http://127.0.0.1:7777");
        assert_eq!(config.runtime_base_url, "http://127.0.0.1:8080");
        assert_eq!(config.request.root_node_id, "node:workspace:demo");
        assert_eq!(config.request.root_node_kind, "workspace");
        assert_eq!(config.request.role, "implementer");
        assert_eq!(config.request.focus_node_kind, "work_item");
        assert_eq!(
            config.request.requested_scopes,
            vec!["implementation".to_string(), "dependencies".to_string()]
        );
        assert_eq!(config.request.token_budget, 1200);
        assert_eq!(config.request.summary_path, "context-summary.md");
    }

    #[test]
    fn from_env_applies_optional_overrides() {
        let env = required_env()
            .into_iter()
            .chain(
                [
                    ("ROOT_NODE_KIND", "claim"),
                    ("AGENT_ROLE", "reviewer"),
                    ("AGENT_FOCUS_NODE_KIND", "incident"),
                    ("AGENT_SCOPES", "triage,summary"),
                    ("AGENT_TOKEN_BUDGET", "900"),
                    ("AGENT_SUMMARY_PATH", "notes/context.md"),
                ]
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.to_string())),
            )
            .collect::<BTreeMap<_, _>>();

        let config = AppConfig::from_lookup(|key| env.get(key).cloned())
            .expect("config should load overrides");

        assert_eq!(config.request.root_node_kind, "claim");
        assert_eq!(config.request.role, "reviewer");
        assert_eq!(config.request.focus_node_kind, "incident");
        assert_eq!(
            config.request.requested_scopes,
            vec!["triage".to_string(), "summary".to_string()]
        );
        assert_eq!(config.request.token_budget, 900);
        assert_eq!(config.request.summary_path, "notes/context.md");
    }

    fn required_env() -> BTreeMap<String, String> {
        [
            ("KERNEL_GRPC_ENDPOINT", "http://127.0.0.1:7777"),
            ("RUNTIME_BASE_URL", "http://127.0.0.1:8080"),
            ("ROOT_NODE_ID", "node:workspace:demo"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
    }
}
