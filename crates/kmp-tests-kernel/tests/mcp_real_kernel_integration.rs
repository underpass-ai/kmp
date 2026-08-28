#![cfg(feature = "container-tests")]

mod support;

use std::collections::BTreeSet;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use kmp_mcp::{GrpcKernelMcpBackend, KernelMcpServer, KernelMcpToolBackend};
use kmp_mcp_http::auth::{Identity, TokenVerifier, VerifyFuture};
use kmp_mcp_http::config::HttpGatewayConfig;
use kmp_mcp_http::{AppState, router};
use kmp_tests_shared::seed::kernel_data::{
    DECISION_DETAIL, DECISION_ID, DECISION_KIND, DEVELOPER_ROLE, HAS_TASK_RELATION, ROOT_NODE_ID,
    TASK_ID,
};
use serde_json::{Value, json};
use tower::ServiceExt;
use url::Url;

use crate::support::seeded_kernel_fixture::SeededKernelFixture;

#[derive(Clone)]
struct ParityVerifier;

impl TokenVerifier for ParityVerifier {
    fn verify<'a>(&'a self, _token: &'a str) -> VerifyFuture<'a> {
        Box::pin(async {
            Ok(Identity {
                subject: "parity-test".to_string(),
                workspace: Some("contract".to_string()),
                scopes: BTreeSet::from([
                    "kmp:read".to_string(),
                    "kmp:write".to_string(),
                    "kmp:inspect:raw".to_string(),
                    "kmp:all-abouts".to_string(),
                ]),
                abouts: BTreeSet::from(["*".to_string()]),
                scope_ids: BTreeSet::from(["*".to_string()]),
                ref_prefixes: BTreeSet::from(["*".to_string()]),
            })
        })
    }
}

#[tokio::test]
async fn grpc_mcp_semantic_parity() -> Result<(), Box<dyn Error + Send + Sync>> {
    let fixture = SeededKernelFixture::start().await?;
    let endpoint = fixture.grpc_endpoint().to_string();
    let direct = GrpcKernelMcpBackend::new(
        endpoint.clone(),
        kmp_mcp::KernelMcpGrpcTlsConfig::disabled(),
    );
    let stdio = KernelMcpServer::grpc(endpoint.clone());
    let http = parity_http_app(KernelMcpServer::grpc(endpoint));
    let embedded_dir = tempfile::tempdir()?;
    let embedded = KernelMcpServer::embedded(embedded_dir.path()).map_err(std::io::Error::other)?;
    direct
        .call_tool("kmp_ingest", &parity_seed_arguments())
        .await
        .unwrap_or_else(|error| panic!("parity seed failed: {error:?}"));
    let embedded_seed = call_tool(&embedded, 0, "kmp_ingest", parity_seed_arguments()).await;
    assert_tool_success(&embedded_seed);

    let cases = vec![
        (
            "kmp_ingest",
            json!({
                "about":"project:parity-preview",
                "memory":{
                    "dimensions":[{"id":"timeline:parity","kind":"timeline"}],
                    "entries":[{"id":"project:parity-preview:observation:parity","kind":"observation","text":"parity",
                        "coordinates":[{"dimension":"timeline","scope_id":"timeline:parity","sequence":1}]}]
                },
                "idempotency_key":"parity-dry-run", "dry_run":true
            }),
        ),
        (
            "kmp_wake",
            json!({"about":"project:parity-live","budget":{"detail":"compact","max_bytes":10000}}),
        ),
        ("kmp_wake", json!({"about":"project:parity-live","depth":2})),
        (
            "kmp_ask",
            json!({"about":"project:parity-live","question":"What changed in the parity fixture?","budget":{"detail":"balanced","max_bytes":10000}}),
        ),
        (
            "kmp_ask",
            json!({
                "about":"project:parity-live",
                "question":"What changed in the parity fixture?",
                "budget":{"detail":"full","max_bytes":100000,"max_entries":1}
            }),
        ),
        (
            "kmp_goto",
            json!({"about":"project:parity-live","at":{"ref":"project:parity-live:observation:parity-after"}}),
        ),
        (
            "kmp_near",
            json!({"about":"project:parity-live","around":{"ref":"project:parity-live:observation:parity-before"}}),
        ),
        (
            "kmp_rewind",
            json!({"about":"project:parity-live","from":{"ref":"project:parity-live:observation:parity-after"}}),
        ),
        (
            "kmp_forward",
            json!({"about":"project:parity-live","from":{"ref":"project:parity-live:observation:parity-before"}}),
        ),
        (
            "kmp_trace",
            json!({"about":"project:parity-live","from":"project:parity-live:observation:parity-after","to":"project:parity-live:observation:parity-before"}),
        ),
        (
            "kmp_inspect",
            json!({"about":"project:parity-live","ref":"project:parity-live:observation:parity-after","include":{"details":true}}),
        ),
    ];

    for (index, (tool, arguments)) in cases.into_iter().enumerate() {
        let direct_result = direct
            .call_tool(tool, &arguments)
            .await
            .unwrap_or_else(|error| panic!("direct {tool} failed: {error:?}"));
        let stdio_result = call_tool(&stdio, index as u64 + 1, tool, arguments.clone()).await;
        let http_result = call_http_tool(&http, index as u64 + 1, tool, arguments.clone()).await;
        let embedded_result = call_tool(&embedded, index as u64 + 1, tool, arguments).await;
        assert_eq!(
            stdio_result["result"], direct_result,
            "stdio semantic result diverged for {tool}"
        );
        assert_eq!(
            http_result["result"], direct_result,
            "HTTP semantic result diverged for {tool}"
        );
        assert_eq!(
            embedded_result["result"], direct_result,
            "embedded semantic result diverged for {tool}"
        );
    }

    let first_page_arguments = json!({
        "about":"project:parity-live",
        "depth":5,
        "budget":{"tokens":30000,"detail":"full","max_bytes":100000},
        "page":{"entries":4}
    });
    let mut unpaged_arguments = first_page_arguments.clone();
    unpaged_arguments
        .as_object_mut()
        .expect("recall arguments")
        .remove("page");
    let direct_unpaged = direct
        .call_tool("kmp_wake", &unpaged_arguments)
        .await
        .expect("direct unpaged recall");
    let complete_items = recall_item_keys(&direct_unpaged["structuredContent"]);

    let mut page_arguments = first_page_arguments.clone();
    let mut expected_offset = 0_u64;
    let mut expected_total = None;
    let mut paged_items = BTreeSet::new();
    let mut seen_cursors = BTreeSet::new();
    loop {
        let request_id = 30 + expected_offset;
        let direct_page = direct
            .call_tool("kmp_wake", &page_arguments)
            .await
            .expect("direct recall page");
        let stdio_page = call_tool(&stdio, request_id, "kmp_wake", page_arguments.clone()).await;
        let http_page = call_http_tool(&http, request_id, "kmp_wake", page_arguments.clone()).await;
        let embedded_page =
            call_tool(&embedded, request_id, "kmp_wake", page_arguments.clone()).await;
        assert_eq!(stdio_page["result"], direct_page);
        assert_eq!(http_page["result"], direct_page);
        assert_eq!(embedded_page["result"], direct_page);

        let content = &direct_page["structuredContent"];
        paged_items.extend(recall_item_keys(content));
        let page = content
            .pointer("/projection/page")
            .expect("recall projection page metadata");
        let offset = page["offset"].as_u64().expect("page offset");
        let returned = page["returned"].as_u64().expect("page returned");
        let total = page["total"].as_u64().expect("page total");
        assert_eq!(offset, expected_offset, "recall pages must be contiguous");
        assert!(returned > 0, "a continuation page must make progress");
        assert_eq!(*expected_total.get_or_insert(total), total);
        expected_offset += returned;

        if page["has_more"] == Value::Bool(false) {
            assert!(page["next_cursor"].is_null());
            assert_eq!(expected_offset, total, "all eligible items were traversed");
            break;
        }
        let cursor = page["next_cursor"]
            .as_str()
            .expect("non-final page cursor")
            .to_string();
        assert!(
            seen_cursors.insert(cursor.clone()),
            "a continuation cursor must not repeat"
        );
        page_arguments["page"]["cursor"] = Value::String(cursor);
    }
    assert_eq!(
        paged_items, complete_items,
        "full cursor traversal must reconstruct the same eligible proof"
    );

    let cursor = first_page_cursor(&direct, &first_page_arguments).await;

    assert_error_code_parity(
        &direct,
        &stdio,
        &http,
        &embedded,
        "kmp_wake",
        json!({}),
        "invalid_argument",
    )
    .await;

    let unavailable_endpoint = "http://127.0.0.1:9".to_string();
    let unavailable_direct = GrpcKernelMcpBackend::new(
        unavailable_endpoint.clone(),
        kmp_mcp::KernelMcpGrpcTlsConfig::disabled(),
    );
    let unavailable_stdio = KernelMcpServer::grpc(unavailable_endpoint.clone());
    let unavailable_http = parity_http_app(KernelMcpServer::grpc(unavailable_endpoint));
    assert_remote_error_code_parity(
        &unavailable_direct,
        &unavailable_stdio,
        &unavailable_http,
        "kmp_wake",
        json!({"about":"project:parity-live"}),
        "unavailable",
    )
    .await;
    assert_error_code_parity(
        &direct,
        &stdio,
        &http,
        &embedded,
        "kmp_goto",
        json!({"about":"project:parity-live","at":{"ref":"missing:temporal-ref"}}),
        "invalid_argument",
    )
    .await;
    assert_error_code_parity(
        &direct,
        &stdio,
        &http,
        &embedded,
        "kmp_inspect",
        json!({"about":"project:parity-live","ref":"project:parity-live:missing:parity-ref"}),
        "not_found",
    )
    .await;
    let mut stale_arguments = first_page_arguments;
    stale_arguments["role"] = Value::String("a-changed-selection".to_string());
    stale_arguments["page"]["cursor"] = Value::String(cursor.to_string());
    assert_error_code_parity(
        &direct,
        &stdio,
        &http,
        &embedded,
        "kmp_wake",
        stale_arguments,
        "invalid_argument",
    )
    .await;

    let write_arguments = json!({
        "about":"project:parity-write",
        "intent":"record_observation",
        "actor":"parity-test",
        "observed_at":"2026-08-25T00:00:00Z",
        "scope":{"process":"parity"},
        "current":{"kind":"observation","summary":"writer parity","evidence":"deterministic fixture"},
        "connect_to":[{"ref":"project:parity-write","rel":"contains","class":"structural"}],
        "idempotency_key":"parity-write-dry-run",
        "options":{"dry_run":true}
    });
    let stdio_write = call_tool(&stdio, 20, "kmp_write_memory", write_arguments.clone()).await;
    let http_write = call_http_tool(&http, 20, "kmp_write_memory", write_arguments.clone()).await;
    let embedded_write = call_tool(&embedded, 20, "kmp_write_memory", write_arguments).await;
    assert_eq!(stdio_write["result"], http_write["result"]);
    assert_eq!(stdio_write["result"], embedded_write["result"]);
    assert_eq!(
        stdio_write.pointer("/result/structuredContent/ingest_preview/dry_run"),
        Some(&Value::Bool(true)),
        "writer helper must compile to canonical dry-run Ingest"
    );

    fixture.shutdown().await?;
    Ok(())
}

async fn first_page_cursor(direct: &GrpcKernelMcpBackend, arguments: &Value) -> String {
    direct
        .call_tool("kmp_wake", arguments)
        .await
        .expect("direct first page")
        .pointer("/structuredContent/projection/page/next_cursor")
        .and_then(Value::as_str)
        .expect("bounded page should expose a continuation cursor")
        .to_string()
}

fn recall_item_keys(content: &Value) -> BTreeSet<String> {
    [
        "/wake/current_state",
        "/wake/causal_spine",
        "/wake/open_loops",
        "/wake/next_actions",
        "/wake/guardrails",
        "/proof/evidence",
        "/proof/path",
        "/proof/missing",
    ]
    .into_iter()
    .flat_map(|path| {
        content
            .pointer(path)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(move |value| format!("{path}:{}", value))
    })
    .collect()
}

async fn assert_error_code_parity(
    direct: &GrpcKernelMcpBackend,
    stdio: &KernelMcpServer,
    http: &Router,
    embedded: &KernelMcpServer,
    tool: &str,
    arguments: Value,
    expected: &str,
) {
    let direct_error = direct
        .call_tool(tool, &arguments)
        .await
        .expect_err("direct call should fail");
    assert_eq!(direct_error.code.as_str(), expected);
    let stdio_error = call_tool(stdio, 40, tool, arguments.clone()).await;
    let http_error = call_http_tool(http, 40, tool, arguments.clone()).await;
    let embedded_error = call_tool(embedded, 40, tool, arguments).await;
    for (path, response) in [
        ("stdio", stdio_error),
        ("http", http_error),
        ("embedded", embedded_error),
    ] {
        assert_eq!(
            response.pointer("/result/structuredContent/error/code"),
            Some(&Value::String(expected.to_string())),
            "{path} error code diverged for {tool}"
        );
    }
}

async fn assert_remote_error_code_parity(
    direct: &GrpcKernelMcpBackend,
    stdio: &KernelMcpServer,
    http: &Router,
    tool: &str,
    arguments: Value,
    expected: &str,
) {
    let direct_error = direct
        .call_tool(tool, &arguments)
        .await
        .expect_err("direct remote call should fail");
    assert_eq!(direct_error.code.as_str(), expected);
    let stdio_error = call_tool(stdio, 50, tool, arguments.clone()).await;
    let http_error = call_http_tool(http, 50, tool, arguments).await;
    for (path, response) in [("stdio", stdio_error), ("http", http_error)] {
        assert_eq!(
            response.pointer("/result/structuredContent/error/code"),
            Some(&Value::String(expected.to_string())),
            "{path} remote error code diverged for {tool}"
        );
    }
}

fn parity_seed_arguments() -> Value {
    json!({
        "about":"project:parity-live",
        "memory":{
            "dimensions":[{"id":"timeline:parity-live","kind":"timeline"}],
            "entries":[
                {"id":"project:parity-live:observation:parity-before","kind":"observation","text":"Parity had one transport.",
                 "coordinates":[{"dimension":"timeline","scope_id":"timeline:parity-live","occurred_at":"2026-08-25T00:00:00Z","ingested_at":"2026-08-25T00:05:00Z","sequence":1}]},
                {"id":"project:parity-live:observation:parity-after","kind":"observation","text":"Parity now covers direct gRPC, stdio MCP, and HTTP MCP.",
                 "coordinates":[{"dimension":"timeline","scope_id":"timeline:parity-live","occurred_at":"2026-08-25T00:01:00Z","ingested_at":"2026-08-25T00:05:00Z","sequence":2}]},
                {"id":"project:parity-live:observation:parity-proof-1","kind":"observation","text":"Compact projection remained bounded.",
                 "coordinates":[{"dimension":"timeline","scope_id":"timeline:parity-live","occurred_at":"2026-08-25T00:02:00Z","ingested_at":"2026-08-25T00:05:00Z","sequence":3}]},
                {"id":"project:parity-live:observation:parity-proof-2","kind":"observation","text":"Balanced projection retained evidence.",
                 "coordinates":[{"dimension":"timeline","scope_id":"timeline:parity-live","occurred_at":"2026-08-25T00:03:00Z","ingested_at":"2026-08-25T00:05:00Z","sequence":4}]},
                {"id":"project:parity-live:observation:parity-proof-3","kind":"observation","text":"Full projection retained relation why.",
                 "coordinates":[{"dimension":"timeline","scope_id":"timeline:parity-live","occurred_at":"2026-08-25T00:04:00Z","ingested_at":"2026-08-25T00:05:00Z","sequence":5}]}
            ],
            "relations":[
                {"from":"project:parity-live:observation:parity-after","to":"project:parity-live:observation:parity-before","rel":"supersedes","class":"evidential","why":"The later observation records the expanded transport matrix.","evidence":"The same test invokes all three paths.","confidence":"high"},
                {"from":"project:parity-live:observation:parity-proof-1","to":"project:parity-live:observation:parity-after","rel":"follows","class":"procedural","why":"The compact check ran after base parity.","evidence":"The parity test sequence records this order.","confidence":"high"},
                {"from":"project:parity-live:observation:parity-proof-2","to":"project:parity-live:observation:parity-proof-1","rel":"follows","class":"procedural","why":"The balanced check followed compact.","evidence":"The parity test sequence records this order.","confidence":"high"},
                {"from":"project:parity-live:observation:parity-proof-3","to":"project:parity-live:observation:parity-proof-2","rel":"follows","class":"procedural","why":"The full check followed balanced.","evidence":"The parity test sequence records this order.","confidence":"high"}
            ],
            "evidence":[
                {"id":"evidence:project:parity-live:parity-live","supports":["project:parity-live:observation:parity-after"],"text":"The semantic results matched exactly.","source":"grpc_mcp_semantic_parity"},
                {"id":"evidence:project:parity-live:parity-compact","supports":["project:parity-live:observation:parity-proof-1"],"text":"Compact stayed under the byte limit.","source":"grpc_mcp_semantic_parity"},
                {"id":"evidence:project:parity-live:parity-balanced","supports":["project:parity-live:observation:parity-proof-2"],"text":"Balanced retained cited evidence.","source":"grpc_mcp_semantic_parity"},
                {"id":"evidence:project:parity-live:parity-full","supports":["project:parity-live:observation:parity-proof-3"],"text":"Full retained the relation rationale.","source":"grpc_mcp_semantic_parity"}
            ]
        },
        "provenance":{"source_kind":"agent","source_agent":"grpc_mcp_semantic_parity","observed_at":"2026-08-25T00:01:00Z"},
        "idempotency_key":"grpc-mcp-semantic-parity-seed"
    })
}

fn parity_http_app(server: KernelMcpServer) -> Router {
    let config = HttpGatewayConfig {
        bind_addr: "127.0.0.1:0".parse().expect("address"),
        public_url: Url::parse("https://kmp.example/mcp").expect("public URL"),
        issuer: Url::parse("https://id.example/").expect("issuer"),
        audience: "https://kmp.example/mcp".to_string(),
        jwks_uri: Some(Url::parse("https://id.example/jwks").expect("JWKS")),
        allowed_origins: BTreeSet::new(),
        request_timeout: Duration::from_secs(20),
        max_body_bytes: 1024 * 1024,
        require_grpc_mtls: false,
    };
    router(AppState::new(config, server, Arc::new(ParityVerifier)))
}

async fn call_http_tool(app: &Router, id: u64, name: &str, arguments: Value) -> Value {
    let request = json!({
        "jsonrpc":"2.0", "id":id, "method":"tools/call",
        "params":{"name":name,"arguments":arguments}
    });
    let response = app
        .clone()
        .oneshot(
            Request::post("/mcp")
                .header("content-type", "application/json")
                .header("authorization", "Bearer parity-token")
                .body(Body::from(request.to_string()))
                .expect("HTTP request"),
        )
        .await
        .expect("HTTP response");
    assert_eq!(response.status(), StatusCode::OK, "HTTP {name} failed");
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .expect("HTTP body");
    serde_json::from_slice(&bytes).expect("HTTP JSON-RPC response")
}

#[tokio::test]
async fn mcp_tools_read_from_live_kernel_grpc_server() -> Result<(), Box<dyn Error + Send + Sync>> {
    let fixture = SeededKernelFixture::start().await?;

    let result = async {
        let server = KernelMcpServer::grpc(fixture.grpc_endpoint().to_string());

        let initialize = call_json_rpc(
            &server,
            1,
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "kernel-container-smoke",
                    "version": "0.1.0"
                }
            }),
        )
        .await;
        assert_eq!(
            initialize.pointer("/result/metadata/backend"),
            Some(&Value::String("grpc".to_string()))
        );

        let ingest = call_tool(
            &server,
            2,
            "kmp_ingest",
            json!({
                "about": "question:mcp-ingest-smoke",
                "memory": {
                    "dimensions": [
                        {
                            "id": "conversation:mcp-ingest-smoke",
                            "kind": "conversation",
                            "title": "MCP ingest smoke"
                        }
                    ],
                    "entries": [
                        {
                            "id": "question:mcp-ingest-smoke:claim:mcp-ingest-before",
                            "kind": "claim",
                            "text": "The MCP adapter should first prove memory can be submitted.",
                            "coordinates": [
                                {
                                    "dimension": "conversation",
                                    "scope_id": "conversation:mcp-ingest-smoke",
                                    "sequence": 1,
                                    "occurred_at": "2026-05-04T10:00:00Z"
                                }
                            ]
                        },
                        {
                            "id": "question:mcp-ingest-smoke:claim:mcp-ingest-after",
                            "kind": "claim",
                            "text": "The MCP adapter can submit memory through the live KernelMemoryService.",
                            "coordinates": [
                                {
                                    "dimension": "conversation",
                                    "scope_id": "conversation:mcp-ingest-smoke",
                                    "sequence": 2,
                                    "occurred_at": "2026-05-04T10:05:00Z"
                                }
                            ]
                        }
                    ],
                    "relations": [
                        {
                            "from": "question:mcp-ingest-smoke:claim:mcp-ingest-after",
                            "to": "question:mcp-ingest-smoke:claim:mcp-ingest-before",
                            "rel": "supersedes",
                            "class": "evidential",
                            "why": "The later smoke claim proves the intended capability.",
                            "confidence": "high"
                        }
                    ],
                    "evidence": [
                        {
                            "id": "evidence:question:mcp-ingest-smoke:current",
                            "supports": ["question:mcp-ingest-smoke:claim:mcp-ingest-after"],
                            "text": "The live smoke accepted kmp_ingest over gRPC.",
                            "source": "mcp_real_kernel_integration"
                        }
                    ]
                },
                "provenance": {
                    "source_kind": "agent",
                    "source_agent": "mcp-real-kernel-smoke",
                    "observed_at": "2026-05-04T10:00:00Z",
                    "correlation_id": "corr:mcp-ingest-smoke",
                    "causation_id": "test:mcp-real-kernel-smoke"
                },
                "idempotency_key": "ingest:mcp-real-kernel-smoke:1"
            }),
        )
        .await;
        assert_tool_success(&ingest);
        let ingest_content = structured_content(&ingest);
        assert_eq!(
            ingest_content.pointer("/memory/about"),
            Some(&Value::String("question:mcp-ingest-smoke".to_string()))
        );
        assert_eq!(
            ingest_content.pointer("/memory/accepted/entries"),
            Some(&Value::from(2))
        );
        assert_eq!(
            ingest_content.pointer("/memory/accepted/evidence"),
            Some(&Value::from(1))
        );
        assert_eq!(
            ingest_content.pointer("/memory/read_after_write_ready"),
            Some(&Value::Bool(true))
        );

        let ingested_wake = call_tool(
            &server,
            3,
            "kmp_wake",
            json!({
                "about": "question:mcp-ingest-smoke",
                "role": "memory",
                "intent": "read back the memory written through MCP",
                "depth": 2,
                "budget": {
                    "tokens": 30000,
                    "max_bytes": 100000,
                    "detail": "full"
                }
            }),
        )
        .await;
        assert_tool_success(&ingested_wake);
        let ingested_wake_content = structured_content(&ingested_wake);
        assert_eq!(
            ingested_wake_content.pointer("/projection/page/has_more"),
            Some(&Value::Bool(false)),
            "the explicit full integration budget should materialize the complete proof"
        );
        assert_array_contains_relation(
            ingested_wake_content,
            "/proof/path",
            "question:mcp-ingest-smoke",
            "question:mcp-ingest-smoke:claim:mcp-ingest-after",
            "records",
        );
        assert_array_contains_relation(
            ingested_wake_content,
            "/proof/path",
            "evidence:question:mcp-ingest-smoke:current",
            "question:mcp-ingest-smoke:claim:mcp-ingest-after",
            "supports",
        );
        assert_array_contains_evidence(
            ingested_wake_content,
            "/proof/evidence",
            "mcp_real_kernel_integration",
        );

        let temporal_forward = call_tool(
            &server,
            31,
            "kmp_forward",
            json!({
                "about": "question:mcp-ingest-smoke",
                "from": {
                    "ref": "question:mcp-ingest-smoke:claim:mcp-ingest-before"
                },
                "dimensions": {
                    "mode": "only",
                    "include": ["conversation"]
                },
                "limit": {
                    "entries": 5
                },
                "depth": 3
            }),
        )
        .await;
        assert_tool_success(&temporal_forward);
        let temporal_forward_content = structured_content(&temporal_forward);
        assert_eq!(
            temporal_forward_content.pointer("/temporal/direction"),
            Some(&Value::String("forward".to_string()))
        );
        assert_array_contains_entry(
            temporal_forward_content,
            "/entries",
            "question:mcp-ingest-smoke:claim:mcp-ingest-after",
        );

        let ingested_ask = call_tool(
            &server,
            32,
            "kmp_ask",
            json!({
                "about": "question:mcp-ingest-smoke",
                "question": "What proved the MCP ingest path?",
                "dimensions": {
                    "mode": "only",
                    "include": ["conversation"]
                },
                "depth": 3,
                "budget": {
                    "tokens": 2048
                }
            }),
        )
        .await;
        assert_tool_success(&ingested_ask);
        let ingested_ask_content = structured_content(&ingested_ask);
        assert_eq!(
            ingested_ask_content.pointer("/answer"),
            Some(&Value::String(
                "Retrieved for this question by term overlap; read proof.evidence and judge whether it answers:\n- question:mcp-ingest-smoke:claim:mcp-ingest-after [detail:evidence:question:mcp-ingest-smoke:current]\n- question:mcp-ingest-smoke:claim:mcp-ingest-after [entry:question:mcp-ingest-smoke:claim:mcp-ingest-after]"
                    .to_string()
            ))
        );
        assert_eq!(
            ingested_ask_content.pointer("/because/0/ref"),
            Some(&Value::String(
                "detail:evidence:question:mcp-ingest-smoke:current".to_string()
            ))
        );
        assert_array_contains_evidence(
            ingested_ask_content,
            "/proof/evidence",
            "mcp_real_kernel_integration",
        );

        let wake = call_tool(
            &server,
            4,
            "kmp_wake",
            json!({
                "about": ROOT_NODE_ID,
                "role": DEVELOPER_ROLE,
                "intent": "continue from the seeded kernel memory",
                "depth": 2,
                "budget": {
                    "tokens": 2048
                }
            }),
        )
        .await;
        assert_tool_success(&wake);
        let wake_content = structured_content(&wake);
        assert_non_empty_string(wake_content, "/summary");
        assert_non_empty_array(wake_content, "/wake/current_state");
        assert_array_contains_evidence(wake_content, "/proof/evidence", ROOT_NODE_ID);

        let ask = call_tool(
            &server,
            5,
            "kmp_ask",
            json!({
                "about": ROOT_NODE_ID,
                "question": "Which seeded decision should the next agent inspect?",
                "depth": 2,
                "budget": {
                    "tokens": 2048
                }
            }),
        )
        .await;
        assert_tool_success(&ask);
        let ask_content = structured_content(&ask);
        assert_eq!(
            ask_content.pointer("/answer"),
            Some(&Value::String("UNKNOWN".to_string()))
        );
        assert!(
            ask_content
                .pointer("/because")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty),
            "seeded node-centric context should not become a KMP Ask answer"
        );

        let trace = call_tool(
            &server,
            6,
            "kmp_trace",
            json!({
                "about": ROOT_NODE_ID,
                "from": ROOT_NODE_ID,
                "to": TASK_ID,
                "role": DEVELOPER_ROLE,
                "budget": {
                    "tokens": 1024
                }
            }),
        )
        .await;
        assert_tool_success(&trace);
        let trace_content = structured_content(&trace);
        assert_non_empty_string(trace_content, "/summary");
        assert_array_contains_relation(
            trace_content,
            "/trace",
            ROOT_NODE_ID,
            TASK_ID,
            HAS_TASK_RELATION,
        );

        let inspect = call_tool(
            &server,
            7,
            "kmp_inspect",
            json!({
                "about": ROOT_NODE_ID,
                "ref": DECISION_ID,
                "include": {
                    "details": true
                }
            }),
        )
        .await;
        assert_tool_success(&inspect);
        let inspect_content = structured_content(&inspect);
        assert_eq!(
            inspect_content.pointer("/object/ref"),
            Some(&Value::String(DECISION_ID.to_string()))
        );
        assert_eq!(
            inspect_content.pointer("/object/kind"),
            Some(&Value::String(DECISION_KIND.to_string()))
        );
        assert_eq!(
            inspect_content.pointer("/object/text"),
            Some(&Value::String(DECISION_DETAIL.to_string())),
            "include.details exposes the inspected object's live Valkey detail"
        );
        let self_citation = format!("detail:{DECISION_ID}");
        assert!(
            array_at(inspect_content, "/evidence")
                .iter()
                .all(|value| value.get("id").and_then(Value::as_str)
                    != Some(self_citation.as_str())),
            "the inspected object must not be returned as evidence for itself"
        );

        Ok::<(), Box<dyn Error + Send + Sync>>(())
    }
    .await;

    fixture.shutdown().await?;
    result
}

async fn call_tool(server: &KernelMcpServer, id: u64, name: &str, arguments: Value) -> Value {
    call_json_rpc(
        server,
        id,
        "tools/call",
        json!({
            "name": name,
            "arguments": arguments
        }),
    )
    .await
}

async fn call_json_rpc(server: &KernelMcpServer, id: u64, method: &str, params: Value) -> Value {
    let response = server
        .handle_json_line(
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params
            })
            .to_string(),
        )
        .await
        .expect("JSON-RPC request should produce a response");

    serde_json::from_str(&response).expect("JSON-RPC response should be valid JSON")
}

fn assert_tool_success(response: &Value) {
    assert_eq!(
        response.pointer("/result/isError"),
        Some(&Value::Bool(false)),
        "tool response should be successful: {response}"
    );
    assert!(
        response.pointer("/result/structuredContent").is_some(),
        "successful MCP tool response should include structuredContent"
    );
}

fn structured_content(response: &Value) -> &Value {
    response
        .pointer("/result/structuredContent")
        .expect("MCP response should include structuredContent")
}

fn assert_non_empty_string(value: &Value, pointer: &str) {
    assert!(
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(|text| !text.trim().is_empty())
            .unwrap_or(false),
        "{pointer} should be a non-empty string"
    );
}

fn assert_non_empty_array(value: &Value, pointer: &str) {
    assert!(
        !array_at(value, pointer).is_empty(),
        "{pointer} should be a non-empty array"
    );
}

fn array_at<'a>(value: &'a Value, pointer: &str) -> &'a [Value] {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .expect("JSON pointer should resolve to an array")
}

fn assert_array_contains_relation(value: &Value, pointer: &str, from: &str, to: &str, rel: &str) {
    assert!(
        array_at(value, pointer).iter().any(|entry| {
            entry.get("from").and_then(Value::as_str) == Some(from)
                && entry.get("to").and_then(Value::as_str) == Some(to)
                && entry.get("rel").and_then(Value::as_str) == Some(rel)
        }),
        "{pointer} should contain relation {from} -[{rel}]-> {to}"
    );
}

fn assert_array_contains_entry(value: &Value, pointer: &str, ref_id: &str) {
    assert!(
        array_at(value, pointer)
            .iter()
            .any(|entry| entry.get("ref").and_then(Value::as_str) == Some(ref_id)),
        "{pointer} should contain entry {ref_id}"
    );
}

fn assert_array_contains_evidence(value: &Value, pointer: &str, source: &str) {
    assert!(
        array_at(value, pointer)
            .iter()
            .any(|entry| entry.get("source").and_then(Value::as_str) == Some(source)),
        "{pointer} should contain evidence from {source}"
    );
}
