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

async fn seeded(dir: &std::path::Path) -> KernelMcpServer {
    let server = KernelMcpServer::embedded(dir).expect("server");
    call(&server, 1, "kmp_ingest", json!({"about": "project:rerank", "idempotency_key": "seed",
        "memory": {"dimensions": [{"id": "test", "kind": "task"}], "entries": [
            {"id": "project:rerank:entry:a", "kind": "observation", "text": "The car was fixed by Ana on Monday.",
             "coordinates": [{"dimension": "task", "scope_id": "test", "sequence": 1, "occurred_at": "2026-01-01T00:00:00Z"}]},
            {"id": "project:rerank:entry:b", "kind": "observation", "text": "A technician repaired the automobile's brakes.",
             "coordinates": [{"dimension": "task", "scope_id": "test", "sequence": 2, "occurred_at": "2026-01-01T00:00:00Z"}]}
        ], "relations": [], "evidence": []}})).await;
    server
}

/// Asking to rerank without the store's TypeSafe opt-in sends nothing and
/// answers exactly as an ordinary Ask, with the reason in a warning.
#[tokio::test]
async fn rerank_without_the_typesafe_opt_in_warns_and_answers_as_today() {
    let plain = tempfile::tempdir().expect("dir");
    let baseline = call(
        &seeded(plain.path()).await,
        2,
        "kmp_ask",
        json!({"about": "project:rerank", "question": "Who fixed the car?"}),
    )
    .await;

    let opted = tempfile::tempdir().expect("dir");
    std::fs::write(opted.path().join("rerank.json"), r#"{"pool_size": 8}"#).expect("config");
    let reranked = call(
        &seeded(opted.path()).await,
        2,
        "kmp_ask",
        json!({"about": "project:rerank", "question": "Who fixed the car?"}),
    )
    .await;

    assert!(
        reranked["warnings"]
            .to_string()
            .contains("evidence rerank disabled"),
        "{reranked}"
    );
    assert!(
        reranked["warnings"].to_string().contains("typesafe.json"),
        "{reranked}"
    );
    assert_eq!(reranked["answer"], baseline["answer"]);
    assert_eq!(reranked["because"], baseline["because"]);
}
