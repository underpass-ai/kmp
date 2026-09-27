//! The host that opened this MCP session, as it named itself in
//! `initialize` — its client name and version, nothing that identifies a
//! person or a machine.

use serde_json::Value;

const NAME_CHARS: usize = 64;
const VERSION_CHARS: usize = 32;

/// `clientInfo.name` and `clientInfo.version`, reduced to printable ASCII
/// and bounded, so a host cannot put arbitrary text into the log.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct McpClient {
    pub(crate) name: String,
    pub(crate) version: String,
}

impl McpClient {
    /// The client an `initialize` request declares; `None` when it names
    /// none that survives sanitizing.
    pub(crate) fn from_initialize(request: &Value) -> Option<Self> {
        let info = request.pointer("/params/clientInfo")?;
        let name = sanitized(info.get("name").and_then(Value::as_str)?, NAME_CHARS);
        if name.is_empty() {
            return None;
        }
        let version = info
            .get("version")
            .and_then(Value::as_str)
            .map(|version| sanitized(version, VERSION_CHARS))
            .unwrap_or_default();
        Some(Self { name, version })
    }
}

fn sanitized(text: &str, limit: usize) -> String {
    text.trim()
        .chars()
        .filter(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '@' | '+' | ' ' | ':')
        })
        .take(limit)
        .collect::<String>()
        .trim()
        .to_string()
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
