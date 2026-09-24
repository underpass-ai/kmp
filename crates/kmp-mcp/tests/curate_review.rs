use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn call(server: &KernelMcpServer, id: u64, name: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc":"2.0", "id":id, "method":"tools/call",
        "params":{"name":name, "arguments":arguments}})
    .to_string();
    let response = server.handle_json_line(&line).await.expect("MCP response");
    let value: Value = serde_json::from_str(&response).expect("JSON");
    assert!(value.get("error").is_none(), "{value}");
    assert_ne!(value["result"]["isError"], true, "{value}");
    value["result"]["structuredContent"].clone()
}

fn seed(about: &str, texts: &[&str]) -> Value {
    let entries = texts
        .iter()
        .enumerate()
        .map(|(n, text)| {
            json!({
                "id": format!("{about}:entry:{n}"), "kind": "observation", "text": text,
                "coordinates": [{"dimension": "task", "scope_id": "test", "sequence": n + 1,
                    "occurred_at": "2026-01-01T00:00:00Z"}]
            })
        })
        .collect::<Vec<_>>();
    json!({"about": about, "idempotency_key": format!("seed:{about}"), "memory": {
        "dimensions": [{"id": "test", "kind": "task"}], "entries": entries,
        "relations": [], "evidence": []}})
}

/// Without `typesafe.json` the review still works: kernel pairs come back
/// untyped, nothing is audited, and the frozen review pages by token.
#[tokio::test]
async fn review_without_jev_lists_kernel_pairs_and_freezes_its_pages() {
    let dir = tempfile::tempdir().expect("dir");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    call(
        &server,
        1,
        "kmp_ingest",
        seed(
            "service:alpha",
            &[
                "Ticket #4711 moved the cache to cluster mode.",
                "Latency dropped after #4711 shipped.",
                "The canteen menu was posted.",
            ],
        ),
    )
    .await;
    call(
        &server,
        2,
        "kmp_ingest",
        seed(
            "service:beta",
            &[
                "The Valkey cluster was upgraded by Ana Ruiz.",
                "Ana Ruiz documented the Valkey upgrade.",
                "Quarterly planning closed.",
            ],
        ),
    )
    .await;
    let first = call(
        &server,
        3,
        "kmp_curate",
        json!({"mode": "review", "about": "service:alpha",
        "dimensions": {"scope": "abouts", "abouts": ["service:alpha", "service:beta"]},
        "page": {"entries": 1}}),
    )
    .await;
    assert!(first["jev"].is_null(), "{first}");
    assert!(first["warnings"].to_string().contains("Jev"), "{first}");
    let missing = first["missing"].as_array().expect("missing");
    assert_eq!(missing.len(), 1, "{first}");
    assert!(missing[0]["suggested_rel"].is_null());
    assert_eq!(missing[0]["proposed_by"], "kernel");
    assert!(
        first["page"]["total"].as_u64().expect("total") >= 2,
        "{first}"
    );
    let token = first["review_token"].as_str().expect("token").to_string();
    let cursor = first["page"]["next_cursor"]
        .as_str()
        .expect("more")
        .to_string();
    let second = call(
        &server,
        4,
        "kmp_curate",
        json!({"mode": "review", "about": "service:alpha",
        "review_token": token, "page": {"entries": 1, "cursor": cursor}}),
    )
    .await;
    assert_eq!(second["review_token"], first["review_token"]);
    assert_ne!(second["missing"], first["missing"], "{second}");
}

#[tokio::test]
async fn an_unknown_review_token_asks_for_a_fresh_review() {
    let dir = tempfile::tempdir().expect("dir");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let line =
        json!({"jsonrpc":"2.0", "id":1, "method":"tools/call", "params":{"name":"kmp_curate",
        "arguments":{"mode":"review","about":"service:alpha","review_token":"0".repeat(64)}}})
        .to_string();
    let response = server.handle_json_line(&line).await.expect("MCP response");
    assert!(response.contains("review expired"), "{response}");
}
