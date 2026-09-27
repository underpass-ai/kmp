//! What one MCP session negotiated: the host as it named itself and whether
//! it renders MCP Apps. The stdio server has one session per process; the
//! HTTP gateway keeps one per `Mcp-Session-Id`, or builds one per request
//! from the stateless dialect's `_meta`, so concurrent sessions never read
//! each other's host.

use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use crate::serving::telemetry::McpClient;

#[derive(Debug, Default)]
pub struct McpSession {
    client: RwLock<Option<McpClient>>,
    apps: AtomicBool,
}

impl McpSession {
    /// A session nothing has initialized: no client and no Apps.
    pub fn new() -> Self {
        Self::default()
    }

    /// The session a stateless request declares in `params._meta`
    /// (`io.modelcontextprotocol/clientInfo` and `…/clientCapabilities`).
    pub fn from_request_meta(request: &Value) -> Self {
        let meta = request.pointer("/params/_meta");
        let client = meta
            .and_then(|meta| meta.get("io.modelcontextprotocol/clientInfo"))
            .and_then(McpClient::from_client_info);
        let apps = meta
            .and_then(|meta| meta.get("io.modelcontextprotocol/clientCapabilities"))
            .is_some_and(capabilities_support_apps);
        Self {
            client: RwLock::new(client),
            apps: AtomicBool::new(apps),
        }
    }

    /// Records what an `initialize` request declares; a later one replaces
    /// it, as a host that re-initializes means.
    pub fn initialize(&self, request: &Value) {
        let client = request
            .pointer("/params/clientInfo")
            .and_then(McpClient::from_client_info);
        if let Ok(mut slot) = self.client.write() {
            *slot = client;
        }
        let apps = request
            .pointer("/params/capabilities")
            .is_some_and(capabilities_support_apps);
        self.apps.store(apps, Ordering::SeqCst);
    }

    /// Whether this session negotiated MCP Apps.
    pub fn apps(&self) -> bool {
        self.apps.load(Ordering::SeqCst)
    }

    /// The client name this session declared, if any.
    pub fn client_name(&self) -> Option<String> {
        self.client().map(|client| client.name)
    }

    pub(crate) fn client(&self) -> Option<McpClient> {
        self.client.read().ok().and_then(|client| client.clone())
    }
}

fn capabilities_support_apps(capabilities: &Value) -> bool {
    capabilities
        .pointer("/extensions/io.modelcontextprotocol~1ui/mimeTypes")
        .and_then(Value::as_array)
        .is_some_and(|types| {
            types
                .iter()
                .any(|value| value.as_str() == Some(crate::contract::MCP_APP_MIME))
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::McpSession;

    #[test]
    fn initialize_records_the_client_and_apps_and_a_later_one_replaces_them() {
        let session = McpSession::new();
        assert!(!session.apps());
        assert_eq!(session.client_name(), None);

        session.initialize(&json!({"params": {
            "clientInfo": {"name": "codex", "version": "1"},
            "capabilities": {"extensions": {"io.modelcontextprotocol/ui": {
                "mimeTypes": [crate::contract::MCP_APP_MIME]}}}
        }}));
        assert!(session.apps());
        assert_eq!(session.client_name().as_deref(), Some("codex"));

        session.initialize(&json!({"params": {"capabilities": {}}}));
        assert!(!session.apps());
        assert_eq!(session.client_name(), None);
    }

    #[test]
    fn a_stateless_request_declares_its_session_in_meta() {
        let session = McpSession::from_request_meta(&json!({"params": {"_meta": {
            "io.modelcontextprotocol/clientInfo": {"name": "claude-code", "version": "2"},
            "io.modelcontextprotocol/clientCapabilities": {"extensions": {
                "io.modelcontextprotocol/ui": {"mimeTypes": [crate::contract::MCP_APP_MIME]}}}
        }}}));
        assert!(session.apps());
        assert_eq!(session.client_name().as_deref(), Some("claude-code"));

        let bare = McpSession::from_request_meta(&json!({"params": {}}));
        assert!(!bare.apps());
        assert_eq!(bare.client_name(), None);
    }
}
