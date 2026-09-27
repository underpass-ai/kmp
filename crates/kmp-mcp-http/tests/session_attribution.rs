//! Concurrent HTTP sessions keep what each host negotiated: its client name
//! in the call log and its MCP Apps surface. Its own test binary, with a
//! global subscriber, so no other test races it for the log.

use std::collections::BTreeSet;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderValue, Request, StatusCode};
use axum::response::Response;
use kmp_mcp::{KernelMcpServer, McpSession};
use kmp_mcp_http::auth::{Identity, TokenVerifier, VerifyFuture};
use kmp_mcp_http::config::HttpGatewayConfig;
use kmp_mcp_http::{AppState, router};
use serde_json::{Value, json};
use tower::ServiceExt;
use url::Url;

const SESSION_HEADER: &str = "mcp-session-id";

struct Verifier;

impl TokenVerifier for Verifier {
    fn verify<'a>(&'a self, _token: &'a str) -> VerifyFuture<'a> {
        Box::pin(async move {
            Ok(Identity {
                subject: "agent-1".to_string(),
                workspace: None,
                scopes: BTreeSet::from([kmp_mcp_http::authorization::READ_SCOPE.to_string()]),
                abouts: BTreeSet::from(["project:kmp".to_string()]),
                scope_ids: BTreeSet::from(["timeline:kmp".to_string()]),
                ref_prefixes: BTreeSet::from(["project:kmp:".to_string()]),
            })
        })
    }
}

fn config() -> HttpGatewayConfig {
    HttpGatewayConfig {
        bind_addr: "127.0.0.1:0".parse().expect("address"),
        public_url: Url::parse("https://kmp.example/mcp").expect("public URL"),
        issuer: Url::parse("https://id.example/").expect("issuer"),
        audience: "https://kmp.example/mcp".to_string(),
        jwks_uri: Some(Url::parse("https://id.example/jwks").expect("JWKS URL")),
        allowed_origins: BTreeSet::new(),
        request_timeout: Duration::from_secs(5),
        max_body_bytes: 1024 * 1024,
        require_grpc_mtls: true,
    }
}

fn post(body: Value) -> Request<Body> {
    Request::post("/mcp")
        .header(AUTHORIZATION, "Bearer test-token")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request")
}

async fn response_json(response: Response) -> Value {
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .expect("response body");
    serde_json::from_slice(&body).expect("JSON response")
}

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("log").extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn initialize(name: &str, apps: bool) -> Value {
    let capabilities = if apps {
        json!({"extensions": {"io.modelcontextprotocol/ui":
            {"mimeTypes": ["text/html;profile=mcp-app"]}}})
    } else {
        json!({})
    };
    json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": capabilities,
        "clientInfo": {"name": name, "version": "1"}}})
}

fn request(body: Value, id: Option<&str>) -> Request<Body> {
    let mut request = post(body);
    if let Some(id) = id {
        request
            .headers_mut()
            .insert(SESSION_HEADER, HeaderValue::from_str(id).expect("id"));
    }
    request
}

#[tokio::test]
async fn two_concurrent_legacy_sessions_are_attributed_to_their_own_host() {
    let log = Captured::default();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(tracing_subscriber::EnvFilter::new("kmp_mcp=info"))
            .with_writer(log.clone())
            .finish(),
    )
    .expect("the only subscriber of this binary");
    let app = router(AppState::new(
        config(),
        KernelMcpServer::fixture(),
        Arc::new(Verifier),
    ));

    let mut ids = Vec::new();
    for (name, apps) in [("codex", true), ("claude-code", false)] {
        let response = app
            .clone()
            .oneshot(request(initialize(name, apps), None))
            .await
            .expect("initialize");
        assert_eq!(response.status(), StatusCode::OK);
        ids.push(
            response.headers()[SESSION_HEADER]
                .to_str()
                .expect("id")
                .to_string(),
        );
    }
    assert_ne!(ids[0], ids[1]);

    let call = |id: u64| {
        json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {
            "name": "kmp_wake", "arguments": {"about": "project:kmp"}}})
    };
    let (a, b, c) = tokio::join!(
        app.clone().oneshot(request(call(2), Some(&ids[0]))),
        app.clone().oneshot(request(call(3), Some(&ids[1]))),
        app.clone().oneshot(request(call(4), None)),
    );
    for response in [a, b, c] {
        assert_eq!(response.expect("call").status(), StatusCode::OK);
    }
    let text = String::from_utf8(log.0.lock().expect("log").clone()).expect("utf8");
    let clients: Vec<Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|line| line["fields"]["event"] == "kmp_mcp_tool")
        .map(|line| line["fields"]["client_name"].clone())
        .collect();
    assert_eq!(clients.len(), 3, "{text}");
    assert!(clients.contains(&json!("codex")));
    assert!(clients.contains(&json!("claude-code")));
    assert!(clients.contains(&Value::Null), "no session id, no client");

    let list = json!({"jsonrpc": "2.0", "id": 5, "method": "tools/list", "params": {}});
    let with_apps = response_json(
        app.clone()
            .oneshot(request(list.clone(), Some(&ids[0])))
            .await
            .expect("list"),
    )
    .await;
    let without = response_json(
        app.clone()
            .oneshot(request(list, Some(&ids[1])))
            .await
            .expect("list"),
    )
    .await;
    assert_eq!(
        with_apps["result"],
        kmp_mcp::kmp_mcp_tools_list_result_with_apps(true)
    );
    assert_eq!(without["result"], kmp_mcp::kmp_mcp_tools_list_result());
}

#[tokio::test]
async fn a_stateless_request_declares_its_own_client() {
    let session = McpSession::from_request_meta(&json!({"params": {"_meta": {
        "io.modelcontextprotocol/clientInfo": {"name": "claude-code", "version": "3"},
        "io.modelcontextprotocol/clientCapabilities": {}
    }}}));
    assert_eq!(session.client_name().as_deref(), Some("claude-code"));
    assert!(!session.apps());
}
