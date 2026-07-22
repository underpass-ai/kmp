//! E3 acceptance: the embedded backend serves KMP tools in-process and
//! memory survives across sessions (fresh-machine criterion analog).

use rehydration_adapter_embedded::RedbQualityTelemetryReader;
use rehydration_mcp::KernelMcpServer;
use serde_json::{Value, json};

fn tool_call(id: u64, name: &str, arguments: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments}
    })
    .to_string()
}

fn ingest_arguments() -> Value {
    json!({
        "about": "question:e3",
        "idempotency_key": "ingest:e3-accept",
        "memory": {
            "dimensions": [{"id": "conversation:s1", "kind": "conversation"}],
            "entries": [
                {
                    "id": "claim:e3",
                    "kind": "claim",
                    "text": "Embedded backend accepted.",
                    "coordinates": [{
                        "dimension": "conversation",
                        "scope_id": "conversation:s1",
                        "occurred_at": "2026-07-22T10:00:00Z",
                        "sequence": 1
                    }]
                },
                {
                    "id": "claim:e3-detail",
                    "kind": "claim",
                    "text": "The in-process backend serves KMP.",
                    "coordinates": [{
                        "dimension": "conversation",
                        "scope_id": "conversation:s1",
                        "occurred_at": "2026-07-22T10:00:01Z",
                        "sequence": 2
                    }]
                }
            ],
            "relations": [{
                "from": "claim:e3",
                "to": "claim:e3-detail",
                "rel": "supports",
                "class": "evidential",
                "why": "The accepted claim answers the checkpoint question.",
                "evidence": "E3 acceptance fixture.",
                "confidence": "high"
            }],
            "evidence": [{
                "id": "evidence:e3",
                "supports": ["claim:e3"],
                "text": "E3 acceptance fixture.",
                "source": "embedded backend test"
            }]
        }
    })
}

async fn call(server: &KernelMcpServer, id: u64, name: &str, arguments: Value) -> Value {
    let response = server
        .handle_json_line(&tool_call(id, name, arguments))
        .await
        .expect("tool call should produce a response");
    let value: Value = serde_json::from_str(&response).expect("response is JSON");
    assert!(
        value.get("error").is_none(),
        "tool `{name}` must not error: {value}"
    );
    value["result"]["structuredContent"].clone()
}

#[tokio::test]
async fn embedded_backend_serves_kmp_tools_and_memory_survives_sessions() {
    let data_dir = tempfile::tempdir().expect("temp data dir");

    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    let ingest = call(&server, 1, "kernel_ingest", ingest_arguments()).await;
    assert_eq!(ingest["memory"]["about"], "question:e3");
    assert_eq!(ingest["memory"]["read_after_write_ready"], true);

    let wake = call(&server, 2, "kernel_wake", json!({"about": "question:e3"})).await;
    let wake_text = wake.to_string();
    assert!(
        wake_text.contains("claim:e3"),
        "wake must surface the ingested entry: {wake_text}"
    );
    drop(server);

    let telemetry = RedbQualityTelemetryReader::open(data_dir.path())
        .expect("quality telemetry journal opens after the session");
    let wake_observations = telemetry
        .query_since(0, Some("kernel_wake"), 10)
        .expect("wake quality observations are queryable");
    assert_eq!(wake_observations.len(), 1);
    assert_eq!(wake_observations[0].root_node_id(), "question:e3");
    drop(telemetry);

    // A brand-new session on the same data dir recovers the memory.
    let second = KernelMcpServer::embedded(data_dir.path()).expect("second session opens");
    let recovered = call(&second, 3, "kernel_wake", json!({"about": "question:e3"})).await;
    assert!(
        recovered.to_string().contains("claim:e3"),
        "second session must recover memory written by the first"
    );
}

#[tokio::test]
async fn embedded_backend_journals_quality_telemetry_for_reads() {
    use rehydration_mcp::{EmbeddedKernelMcpBackend, KernelMcpToolBackend};

    let data_dir = tempfile::tempdir().expect("temp data dir");
    let backend = EmbeddedKernelMcpBackend::open(data_dir.path()).expect("backend opens");

    backend
        .call_tool("kernel_ingest", &ingest_arguments())
        .await
        .expect("ingest succeeds");
    backend
        .call_tool("kernel_wake", &serde_json::json!({"about": "question:e3"}))
        .await
        .expect("wake succeeds");
    backend
        .call_tool(
            "kernel_ask",
            &serde_json::json!({
                "about": "question:e3",
                "question": "What was accepted?"
            }),
        )
        .await
        .expect("ask succeeds");
    backend
        .call_tool(
            "kernel_trace",
            &serde_json::json!({
                "from": "claim:e3",
                "to": "claim:e3-detail"
            }),
        )
        .await
        .expect("trace succeeds");
    drop(backend);

    let telemetry = RedbQualityTelemetryReader::open(data_dir.path()).expect("journal opens");
    let wakes = telemetry
        .query_since(0, Some("kernel_wake"), 10)
        .expect("wake observations query");
    let asks = telemetry
        .query_since(0, Some("kernel_ask"), 10)
        .expect("ask observations query");
    let traces = telemetry
        .query_since(0, Some("kernel_trace"), 10)
        .expect("trace observations query");
    assert_eq!(wakes.len(), 1, "wake must journal one observation");
    assert_eq!(asks.len(), 1, "ask must journal one observation");
    assert_eq!(traces.len(), 1, "trace must journal one observation");
    assert_eq!(wakes[0].root_node_id(), "question:e3");
    assert!(wakes[0].raw_equivalent_tokens() > 0);
}
