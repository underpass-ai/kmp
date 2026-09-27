//! What the server knows about a call's origin for its log line: the host
//! that initialized the session and the store's fingerprint salt.

use std::sync::Arc;

use serde_json::Value;

use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::serving::telemetry::{CallOrigin, FingerprintSalt, McpClient};

impl KernelMcpServer {
    /// Remembers the client an `initialize` names (or that it named none).
    pub(super) fn remember_client(&self, request: &Value) {
        let client = McpClient::from_initialize(request);
        if let Ok(mut slot) = self.mcp_client.write() {
            *slot = client;
        }
    }

    /// The origin of a call, from its arguments as the host sent them.
    pub(super) fn call_origin(&self, name: &str, arguments: &Value) -> CallOrigin {
        let client = self
            .mcp_client
            .read()
            .ok()
            .and_then(|client| client.clone());
        CallOrigin::read(name, arguments, client)
    }

    /// The store's fingerprint salt. `create` reads or creates it on first
    /// use — only after a wake or an ask succeeded, so the store exists;
    /// without it only a salt already loaded is used. A salt that cannot be
    /// had is said once and the calls go unfingerprinted.
    pub(super) fn telemetry_salt(&self, create: bool) -> Option<&FingerprintSalt> {
        if !create {
            return self.telemetry_salt.get()?.as_deref();
        }
        self.telemetry_salt
            .get_or_init(|| {
                let path = self.telemetry_salt_path.as_ref()?;
                match FingerprintSalt::load_or_create(path) {
                    Ok(salt) => Some(Arc::new(salt)),
                    Err(error) => {
                        tracing::warn!(
                            event = "kmp_telemetry_salt",
                            %error,
                            "call fingerprints are off for this process"
                        );
                        None
                    }
                }
            })
            .as_deref()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::serving::kernel_mcp_server::KernelMcpServer;
    use crate::serving::telemetry::TELEMETRY_SALT_FILE;

    #[test]
    fn the_origin_carries_the_initialized_client() {
        let server = KernelMcpServer::fixture();
        assert!(server.call_origin("kmp_ask", &json!({})).client.is_none());

        server
            .remember_client(&json!({"params": {"clientInfo": {"name": "codex", "version": "1"}}}));
        let origin = server.call_origin("kmp_ask", &json!({"continuation": "c"}));
        assert_eq!(origin.client.expect("client").name, "codex");
        assert_eq!(origin.is_continuation, Some(true));
    }

    #[test]
    fn the_salt_is_created_only_when_asked_and_only_where_the_store_is() {
        let fixture = KernelMcpServer::fixture();
        assert!(fixture.telemetry_salt(true).is_none());

        let dir = tempfile::tempdir().expect("dir");
        let mut server = KernelMcpServer::fixture();
        server.telemetry_salt_path = Some(dir.path().join(TELEMETRY_SALT_FILE));
        assert!(server.telemetry_salt(false).is_none());
        assert!(!dir.path().join(TELEMETRY_SALT_FILE).exists());
        assert!(server.telemetry_salt(true).is_some());
        assert!(server.telemetry_salt(false).is_some());
        assert!(dir.path().join(TELEMETRY_SALT_FILE).is_file());

        let mut broken = KernelMcpServer::fixture();
        broken.telemetry_salt_path = Some(dir.path().join("absent").join(TELEMETRY_SALT_FILE));
        assert!(broken.telemetry_salt(true).is_none());
        assert!(broken.telemetry_salt(false).is_none());
    }

    async fn call(server: &KernelMcpServer, id: u64, tool: &str, arguments: Value) -> String {
        let request = json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": tool, "arguments": arguments}});
        server
            .handle_json_line(&request.to_string())
            .await
            .expect("reply")
    }

    #[tokio::test]
    async fn an_embedded_ask_logs_its_origin_and_creates_the_salt_beside_the_store() {
        use crate::serving::telemetry::captured_log::CapturedLog;

        let dir = tempfile::tempdir().expect("dir");
        let server = KernelMcpServer::embedded(dir.path()).expect("server");
        let guide: Vec<Value> = serde_json::from_str(include_str!(
            "../../../../plugins/kmp/guide/guide.requests.json"
        ))
        .expect("guide");
        for (id, arguments) in guide.into_iter().enumerate() {
            call(&server, id as u64, "kmp_ingest", arguments).await;
        }
        assert!(!dir.path().join(TELEMETRY_SALT_FILE).exists());
        let (log, _guard) = CapturedLog::start("kmp_mcp=info");
        server
            .handle_json_line(
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"telemetry-test","version":"9"}}}"#,
            )
            .await
            .expect("initialize");
        let asked = json!({"about": "guide:kmp-agent",
            "question": "How does an agent resume private work after compaction?",
            "page": {"entries": 1}, "budget": {"max_bytes": 200000}});
        let first = call(&server, 2, "kmp_ask", asked.clone()).await;
        let second = call(&server, 2, "kmp_ask", asked).await;
        assert_eq!(first, second, "telemetry never changes the answer");
        let woke = call(
            &server,
            4,
            "kmp_wake",
            json!({"about": "guide:kmp-agent", "intent": "resume", "page": {"entries": 1},
                "budget": {"max_bytes": 200000}}),
        )
        .await;
        let body: Value = serde_json::from_str(&woke).expect("json");
        let next = body
            .pointer("/result/structuredContent/projection/next_action/arguments")
            .cloned()
            .expect("a wake of the whole guide pages");
        assert!(
            next.get("continuation").is_some() || next.pointer("/page/cursor").is_some(),
            "{next}"
        );
        call(&server, 5, "kmp_wake", next).await;

        let salt = dir.path().join(TELEMETRY_SALT_FILE);
        assert!(salt.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&salt).expect("salt").permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let lines = log.events("kmp_mcp_tool");
        assert_eq!(lines.len(), 4, "{lines:?}");
        let fields = &lines[0]["fields"];
        assert_eq!(fields["status"], "success", "{fields}");
        assert_eq!(fields["kmp_move"], "kmp_ask");
        assert_eq!(fields["client_name"], "telemetry-test");
        assert_eq!(fields["is_continuation"], false);
        assert!(fields["subject_fingerprint"].is_string());
        assert_eq!(
            fields["subject_fingerprint"],
            lines[1]["fields"]["subject_fingerprint"]
        );
        assert!(fields["citations"].is_u64());
        assert!(fields["confidence"].is_string());
        let (wake, page) = (&lines[2]["fields"], &lines[3]["fields"]);
        assert_eq!(wake["kmp_move"], "kmp_wake");
        assert_eq!(wake["is_continuation"], false, "{wake}");
        assert!(wake.get("anchored").is_none());
        assert_eq!(page["is_continuation"], true, "{page}");
        assert!(page["subject_fingerprint"].is_string(), "{page}");
        assert_eq!(page["subject_fingerprint"], wake["subject_fingerprint"]);
        assert_ne!(wake["subject_fingerprint"], fields["subject_fingerprint"]);
        for line in &lines {
            assert!(!line.to_string().contains("private work"));
        }
    }

    #[tokio::test]
    async fn a_refused_write_logs_its_feedback_codes_and_fields_only() {
        use crate::serving::telemetry::captured_log::CapturedLog;

        let dir = tempfile::tempdir().expect("dir");
        let server = KernelMcpServer::embedded(dir.path()).expect("server");
        let (log, _guard) = CapturedLog::start("kmp_mcp=info");
        server.remember_client(&json!({"params": {"clientInfo": {"name": "codex-mcp-client"}}}));
        let refused = call(
            &server,
            1,
            "kmp_write_memory",
            json!({"about": "project:telemetry", "observed_at": "2026-09-27T10:00:00Z",
                "idempotency_key": "telemetry:refused",
                "memories": [{"id": "a", "kind": "observation",
                    "summary": "La cafetera privada falla.", "evidence": "Private evidence text."}]}),
        )
        .await;
        let body: Value = serde_json::from_str(&refused).expect("json");
        assert_eq!(body["result"]["isError"], true, "{body}");

        let lines = log.events("kmp_mcp_tool");
        assert_eq!(lines.len(), 1);
        let fields = &lines[0]["fields"];
        assert_eq!(fields["status"], "error");
        assert_eq!(fields["error_kind"], "validation");
        assert_eq!(fields["client_name"], "codex-mcp-client");
        let codes = fields["feedback_codes"].as_str().expect("codes");
        let returned = body["result"]["structuredContent"]["feedback"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default();
        assert_eq!(fields["feedback_count"], returned, "{codes}");
        assert!(!codes.is_empty());
        let text = lines[0].to_string();
        for private in ["cafetera", "Private evidence", "project:telemetry"] {
            assert!(!text.contains(private), "{text}");
        }
        assert!(!dir.path().join(TELEMETRY_SALT_FILE).exists());
    }
}
