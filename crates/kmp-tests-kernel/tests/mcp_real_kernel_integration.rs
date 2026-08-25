#![cfg(feature = "container-tests")]

mod support;

use std::error::Error;

use kmp_mcp::KernelMcpServer;
use kmp_tests_shared::seed::kernel_data::{
    DECISION_DETAIL, DECISION_ID, DECISION_KIND, DEVELOPER_ROLE, HAS_TASK_RELATION, ROOT_NODE_ID,
    TASK_ID,
};
use serde_json::{Value, json};

use crate::support::seeded_kernel_fixture::SeededKernelFixture;

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
                            "id": "claim:mcp-ingest-before",
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
                            "id": "claim:mcp-ingest-after",
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
                            "from": "claim:mcp-ingest-after",
                            "to": "claim:mcp-ingest-before",
                            "rel": "supersedes",
                            "class": "evidential",
                            "why": "The later smoke claim proves the intended capability.",
                            "confidence": "high"
                        }
                    ],
                    "evidence": [
                        {
                            "id": "evidence:mcp-ingest-smoke",
                            "supports": ["claim:mcp-ingest-after"],
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
            "claim:mcp-ingest-after",
            "records",
        );
        assert_array_contains_relation(
            ingested_wake_content,
            "/proof/path",
            "evidence:mcp-ingest-smoke",
            "claim:mcp-ingest-after",
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
                    "ref": "claim:mcp-ingest-before"
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
            "claim:mcp-ingest-after",
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
                "Retrieved for this question by term overlap; read proof.evidence and judge whether it answers: claim:mcp-ingest-after [detail:evidence:mcp-ingest-smoke]"
                    .to_string()
            ))
        );
        assert_eq!(
            ingested_ask_content.pointer("/because/0/ref"),
            Some(&Value::String(
                "detail:evidence:mcp-ingest-smoke".to_string()
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
        assert!(
            array_at(inspect_content, "/evidence")
                .iter()
                .any(|value| value.get("text").and_then(Value::as_str) == Some(DECISION_DETAIL)),
            "kmp_inspect should expose the live Valkey detail"
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
