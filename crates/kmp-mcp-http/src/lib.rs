pub mod adapters;
pub mod application;
pub mod auth;
pub mod authorization;
pub mod config;
pub mod domain;
pub mod ports;
mod protocol;

use std::sync::Arc;

use application::use_cases::authorize_mcp_request::AuthorizeMcpRequest;
use auth::{AuthError, TokenVerifier};
use authorization::AuthorizationError;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, ORIGIN, WWW_AUTHENTICATE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use config::HttpGatewayConfig;
use kmp_mcp::KernelMcpServer;
use protocol::{RequestDialect, add_current_response_metadata, jsonrpc_error, validate_request};
use serde_json::{Value, json};

#[derive(Clone)]
pub struct AppState {
    config: HttpGatewayConfig,
    server: Arc<KernelMcpServer>,
    verifier: Arc<dyn TokenVerifier>,
}

impl AppState {
    pub fn new(
        config: HttpGatewayConfig,
        server: KernelMcpServer,
        verifier: Arc<dyn TokenVerifier>,
    ) -> Self {
        Self {
            config,
            server: Arc::new(server),
            verifier,
        }
    }
}

pub fn router(state: AppState) -> Router {
    let max_body_bytes = state.config.max_body_bytes;
    Router::new()
        .route("/mcp", post(handle_mcp))
        .route("/healthz", get(health))
        .route("/readyz", get(health))
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(protected_resource_metadata),
        )
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok\n")
}

async fn protected_resource_metadata(State(state): State<AppState>) -> Response {
    json_response(
        StatusCode::OK,
        json!({
            "resource": state.config.public_url.as_str(),
            "authorization_servers": [state.config.issuer.as_str()],
            "bearer_methods_supported": ["header"],
            "scopes_supported": [
                authorization::READ_SCOPE,
                authorization::WRITE_SCOPE,
                authorization::RAW_SCOPE,
                authorization::ALL_ABOUTS_SCOPE
            ]
        }),
    )
}

async fn handle_mcp(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    if let Err(reason) = validate_origin(&state, &headers) {
        tracing::warn!(decision = "deny", reason, "HTTP MCP request denied");
        return json_response(
            StatusCode::FORBIDDEN,
            jsonrpc_error(Value::Null, -32003, reason),
        );
    }

    if !headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("application/json")
                || value.to_ascii_lowercase().starts_with("application/json;")
        })
    {
        return json_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            jsonrpc_error(Value::Null, -32600, "Content-Type must be application/json"),
        );
    }

    let request: Value = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                jsonrpc_error(Value::Null, -32700, "invalid JSON-RPC message"),
            );
        }
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let dialect = match validate_request(&headers, &request) {
        Ok(dialect) => dialect,
        Err(error) => {
            return json_response(error.status, jsonrpc_error(id, error.code, &error.message));
        }
    };
    let token = match bearer_token(&headers) {
        Ok(token) => token,
        Err(message) => return unauthorized(&state, &message),
    };
    let identity = match state.verifier.verify(token).await {
        Ok(identity) => identity,
        Err(AuthError::Unauthorized(message)) => return unauthorized(&state, &message),
        Err(AuthError::Unavailable(message)) => {
            tracing::error!(reason = %message, "OIDC verifier unavailable");
            return json_response(
                StatusCode::SERVICE_UNAVAILABLE,
                jsonrpc_error(id, -32004, "authorization service is unavailable"),
            );
        }
    };
    let tool = request_tool(&request);
    let about = request_about(&request);
    if let Err(error) = AuthorizeMcpRequest::execute(&identity, &request) {
        audit(&identity, "deny", tool, about, Some(&error.reason));
        return forbidden(&state, id, error);
    }
    audit(&identity, "allow", tool, about, None);

    let serialized = match serde_json::to_string(&request) {
        Ok(serialized) => serialized,
        Err(_) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                jsonrpc_error(id, -32600, "request cannot be serialized"),
            );
        }
    };
    let result = tokio::time::timeout(
        state.config.request_timeout,
        state.server.handle_json_line(&serialized),
    )
    .await;
    let response = match result {
        Err(_) => {
            tracing::warn!(subject = %identity.subject, tool, "HTTP MCP request deadline exceeded");
            return json_response(
                StatusCode::GATEWAY_TIMEOUT,
                jsonrpc_error(id, -32001, "KMP request deadline exceeded"),
            );
        }
        Ok(None) => return StatusCode::ACCEPTED.into_response(),
        Ok(Some(response)) => match serde_json::from_str::<Value>(&response) {
            Ok(response) => response,
            Err(_) => {
                return json_response(
                    StatusCode::BAD_GATEWAY,
                    jsonrpc_error(id, -32603, "KMP returned an invalid response"),
                );
            }
        },
    };
    let response = if dialect == RequestDialect::Current {
        add_current_response_metadata(response)
    } else {
        response
    };
    let mut response = json_response(StatusCode::OK, response);
    if dialect == RequestDialect::Current {
        response.headers_mut().insert(
            protocol::PROTOCOL_VERSION_HEADER,
            HeaderValue::from_static(protocol::CURRENT_PROTOCOL_VERSION),
        );
    }
    response
}

fn validate_origin<'a>(state: &AppState, headers: &'a HeaderMap) -> Result<(), &'a str> {
    let Some(origin) = headers.get(ORIGIN) else {
        return Ok(());
    };
    let origin = origin.to_str().map_err(|_| "invalid Origin header")?;
    if state.config.allowed_origins.contains(origin) {
        Ok(())
    } else {
        Err("Origin is not allowed")
    }
}

fn bearer_token(headers: &HeaderMap) -> Result<&str, String> {
    let value = headers
        .get(AUTHORIZATION)
        .ok_or_else(|| "bearer token is required".to_string())?
        .to_str()
        .map_err(|_| "authorization header is invalid".to_string())?;
    let (scheme, token) = value
        .split_once(' ')
        .ok_or_else(|| "bearer token is required".to_string())?;
    if !scheme.eq_ignore_ascii_case("bearer") || token.trim().is_empty() || token.contains(' ') {
        return Err("bearer token is required".to_string());
    }
    Ok(token)
}

fn unauthorized(state: &AppState, message: &str) -> Response {
    tracing::warn!(
        decision = "deny",
        reason = message,
        "HTTP MCP authentication denied"
    );
    let mut response = json_response(
        StatusCode::UNAUTHORIZED,
        jsonrpc_error(Value::Null, -32002, "authentication required"),
    );
    let challenge = format!(
        "Bearer resource_metadata=\"{}\"",
        state.config.resource_metadata_url()
    );
    if let Ok(value) = HeaderValue::from_str(&challenge) {
        response.headers_mut().insert(WWW_AUTHENTICATE, value);
    }
    response
}

fn forbidden(state: &AppState, id: Value, error: AuthorizationError) -> Response {
    let mut response = json_response(
        StatusCode::FORBIDDEN,
        jsonrpc_error(
            id,
            -32003,
            "token grant does not authorize this KMP request",
        ),
    );
    if let Some(scope) = error.required_scope {
        let challenge = format!(
            "Bearer error=\"insufficient_scope\", scope=\"{scope}\", resource_metadata=\"{}\"",
            state.config.resource_metadata_url()
        );
        if let Ok(value) = HeaderValue::from_str(&challenge) {
            response.headers_mut().insert(WWW_AUTHENTICATE, value);
        }
    }
    response
}

fn audit(
    identity: &auth::Identity,
    decision: &str,
    tool: &str,
    about: Option<&str>,
    reason: Option<&str>,
) {
    tracing::info!(
        decision,
        subject = %identity.subject,
        workspace = identity.workspace.as_deref().unwrap_or(""),
        tool,
        about = about.unwrap_or(""),
        reason = reason.unwrap_or(""),
        "HTTP MCP authorization decision"
    );
}

fn request_tool(request: &Value) -> &str {
    request
        .pointer("/params/name")
        .and_then(Value::as_str)
        .or_else(|| request.get("method").and_then(Value::as_str))
        .unwrap_or("")
}

fn request_about(request: &Value) -> Option<&str> {
    request
        .pointer("/params/arguments/about")
        .and_then(Value::as_str)
}

fn json_response(status: StatusCode, value: Value) -> Response {
    let mut response = (status, serde_json::to_vec(&value).unwrap_or_default()).into_response();
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use auth::{Identity, VerifyFuture};
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use kmp_mcp::{KernelMcpToolBackend, KernelMcpToolFuture};
    use serde_json::json;
    use tower::ServiceExt;
    use url::Url;

    use super::*;

    #[derive(Clone)]
    struct FakeVerifier {
        result: Result<Identity, AuthError>,
    }

    impl TokenVerifier for FakeVerifier {
        fn verify<'a>(&'a self, _token: &'a str) -> VerifyFuture<'a> {
            let result = self.result.clone();
            Box::pin(async move { result })
        }
    }

    #[derive(Clone)]
    struct CountingBackend(Arc<AtomicUsize>);

    impl KernelMcpToolBackend for CountingBackend {
        fn backend_name(&self) -> &'static str {
            "counting"
        }

        fn call_tool<'a>(
            &'a self,
            name: &'a str,
            _arguments: &'a Value,
        ) -> KernelMcpToolFuture<'a> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(json!({"summary":format!("called {name}")})) })
        }
    }

    #[derive(Clone)]
    struct SlowBackend;

    impl KernelMcpToolBackend for SlowBackend {
        fn backend_name(&self) -> &'static str {
            "slow"
        }

        fn call_tool<'a>(
            &'a self,
            _name: &'a str,
            _arguments: &'a Value,
        ) -> KernelMcpToolFuture<'a> {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(json!({"summary":"too late"}))
            })
        }
    }

    fn identity(scopes: &[&str]) -> Identity {
        Identity {
            subject: "agent-1".to_string(),
            workspace: Some("underpass".to_string()),
            scopes: scopes.iter().map(|scope| (*scope).to_string()).collect(),
            abouts: BTreeSet::from(["project:kmp".to_string()]),
            scope_ids: BTreeSet::from(["timeline:kmp".to_string()]),
            ref_prefixes: BTreeSet::from(["project:kmp:".to_string()]),
        }
    }

    fn config(timeout: Duration) -> HttpGatewayConfig {
        HttpGatewayConfig {
            bind_addr: "127.0.0.1:0".parse().expect("address"),
            public_url: Url::parse("https://kmp.example/mcp").expect("public URL"),
            issuer: Url::parse("https://id.example/").expect("issuer"),
            audience: "https://kmp.example/mcp".to_string(),
            jwks_uri: Some(Url::parse("https://id.example/jwks").expect("JWKS URL")),
            allowed_origins: BTreeSet::from([
                "https://client.example".to_string(),
                "https://kmp.example".to_string(),
            ]),
            request_timeout: timeout,
            max_body_bytes: 1024 * 1024,
            require_grpc_mtls: true,
        }
    }

    fn app_with(identity: Result<Identity, AuthError>, calls: Arc<AtomicUsize>) -> Router {
        router(AppState::new(
            config(Duration::from_secs(1)),
            KernelMcpServer::with_backend(CountingBackend(calls)),
            Arc::new(FakeVerifier { result: identity }),
        ))
    }

    fn request(body: Value) -> Request<Body> {
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

    #[tokio::test]
    async fn lists_the_same_surface_over_legacy_http() {
        let calls = Arc::new(AtomicUsize::new(0));
        let response = app_with(Ok(identity(&[])), calls)
            .oneshot(request(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
            ))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["result"], kmp_mcp::kmp_mcp_tools_list_result());
        assert_eq!(body["result"]["tools"].as_array().expect("tools").len(), 13);
    }

    #[tokio::test]
    async fn current_stateless_http_validates_headers_and_returns_server_info() {
        let calls = Arc::new(AtomicUsize::new(0));
        let body = json!({
            "jsonrpc":"2.0", "id":1, "method":"tools/list",
            "params":{"_meta":{
                "io.modelcontextprotocol/protocolVersion":protocol::CURRENT_PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientCapabilities":{}
            }}
        });
        let request = Request::post("/mcp")
            .header(AUTHORIZATION, "Bearer test-token")
            .header(CONTENT_TYPE, "application/json")
            .header(
                protocol::PROTOCOL_VERSION_HEADER,
                protocol::CURRENT_PROTOCOL_VERSION,
            )
            .header(protocol::METHOD_HEADER, "tools/list")
            .header("accept", "application/json, text/event-stream")
            .body(Body::from(body.to_string()))
            .expect("request");
        let response = app_with(Ok(identity(&[])), calls)
            .oneshot(request)
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[protocol::PROTOCOL_VERSION_HEADER],
            protocol::CURRENT_PROTOCOL_VERSION
        );
        let body = response_json(response).await;
        assert_eq!(
            body.pointer("/result/_meta/io.modelcontextprotocol~1serverInfo/name"),
            Some(&Value::String("underpass-kmp-mcp-http".to_string()))
        );
    }

    #[tokio::test]
    async fn notifications_are_acknowledged_without_a_response_body() {
        let calls = Arc::new(AtomicUsize::new(0));
        let response = app_with(Ok(identity(&[])), calls)
            .oneshot(request(json!({
                "jsonrpc":"2.0", "method":"notifications/initialized", "params":{}
            })))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn request_deadline_bounds_a_slow_kernel_call() {
        let app = router(AppState::new(
            config(Duration::from_millis(1)),
            KernelMcpServer::with_backend(SlowBackend),
            Arc::new(FakeVerifier {
                result: Ok(identity(&[authorization::READ_SCOPE])),
            }),
        ));
        let call = json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"kmp_wake","arguments":{"about":"project:kmp"}}
        });
        let response = app.oneshot(request(call)).await.expect("response");
        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    }

    #[tokio::test]
    async fn authentication_and_authorization_fail_before_backend_call() {
        let calls = Arc::new(AtomicUsize::new(0));
        let app = app_with(Ok(identity(&[])), calls.clone());
        let call = json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"kmp_wake","arguments":{"about":"project:kmp"}}
        });
        let no_token = Request::post("/mcp")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(call.to_string()))
            .expect("request");
        assert_eq!(
            app.clone()
                .oneshot(no_token)
                .await
                .expect("response")
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            app.oneshot(request(call)).await.expect("response").status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn foreign_ingest_refs_are_denied_before_backend_call() {
        let calls = Arc::new(AtomicUsize::new(0));
        let call = json!({
            "jsonrpc":"2.0", "id":2, "method":"tools/call",
            "params":{
                "name":"kmp_ingest",
                "arguments":{
                    "about":"project:kmp",
                    "idempotency_key":"http-auth-cross-about",
                    "memory":{
                        "dimensions":[{"id":"timeline:kmp", "kind":"agentic_process"}],
                        "entries":[{"id":"project:other:entry:1"}]
                    }
                }
            }
        });
        let response = app_with(Ok(identity(&[authorization::WRITE_SCOPE])), calls.clone())
            .oneshot(request(call))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn origin_is_checked_before_token_or_backend() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut denied = request(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}));
        denied
            .headers_mut()
            .insert(ORIGIN, HeaderValue::from_static("https://evil.example"));
        let response = app_with(Ok(identity(&[])), calls.clone())
            .oneshot(denied)
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn allowed_read_reaches_the_shared_mcp_server() {
        let calls = Arc::new(AtomicUsize::new(0));
        let call = json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"kmp_wake","arguments":{"about":"project:kmp"}}
        });
        let response = app_with(Ok(identity(&[authorization::READ_SCOPE])), calls.clone())
            .oneshot(request(call))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn protected_resource_metadata_and_challenge_point_to_the_resource() {
        let calls = Arc::new(AtomicUsize::new(0));
        let app = app_with(Ok(identity(&[])), calls);
        let metadata = app
            .clone()
            .oneshot(
                Request::get("/.well-known/oauth-protected-resource")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(
            response_json(metadata).await["resource"],
            "https://kmp.example/mcp"
        );
        let unauthorized = app
            .oneshot(
                Request::post("/mcp")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}})
                            .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert!(
            unauthorized.headers()[WWW_AUTHENTICATE]
                .to_str()
                .expect("challenge")
                .contains("/.well-known/oauth-protected-resource")
        );
    }
}
