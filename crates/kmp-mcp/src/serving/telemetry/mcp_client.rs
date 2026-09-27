//! The host that opened this MCP session, as it named itself in
//! `initialize` — its client name and version, nothing that identifies a
//! person or a machine.

use serde_json::Value;

use kmp_observability::{CLIENT_NAME_CHARS as NAME_CHARS, CLIENT_VERSION_CHARS as VERSION_CHARS};

/// `clientInfo.name` and `clientInfo.version`, reduced to printable ASCII
/// and bounded, so a host cannot put arbitrary text into the log.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct McpClient {
    pub(crate) name: String,
    pub(crate) version: String,
}

impl McpClient {
    /// The client a `clientInfo` object declares; `None` when it names
    /// none that survives sanitizing.
    pub(crate) fn from_client_info(info: &Value) -> Option<Self> {
        let name =
            kmp_observability::client_label(info.get("name").and_then(Value::as_str)?, NAME_CHARS);
        if name.is_empty() {
            return None;
        }
        let version = info
            .get("version")
            .and_then(Value::as_str)
            .map(|version| kmp_observability::client_label(version, VERSION_CHARS))
            .unwrap_or_default();
        Some(Self { name, version })
    }

    /// The client an `initialize` request declares.
    #[cfg(test)]
    pub(crate) fn from_initialize(request: &Value) -> Option<Self> {
        Self::from_client_info(request.pointer("/params/clientInfo")?)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::McpClient;

    #[test]
    fn reads_the_declared_client_and_bounds_it() {
        let client = McpClient::from_initialize(&json!({
            "params": {"clientInfo": {"name": "claude-code", "version": "2.1.0"}}
        }));
        assert_eq!(
            client,
            Some(McpClient {
                name: "claude-code".into(),
                version: "2.1.0".into()
            })
        );

        let long = "x".repeat(200);
        let odd = McpClient::from_initialize(&json!({
            "params": {"clientInfo": {"name": format!("codex\n\"{long}"), "version": 7}}
        }))
        .expect("client");
        assert!(odd.name.starts_with("codex"));
        assert_eq!(odd.name.len(), 64);
        assert_eq!(odd.version, "");
    }

    #[test]
    fn a_host_that_names_nothing_is_no_client() {
        assert_eq!(McpClient::from_initialize(&json!({"params": {}})), None);
        assert_eq!(
            McpClient::from_initialize(&json!({"params": {"clientInfo": {"name": " \n "}}})),
            None
        );
    }
}
