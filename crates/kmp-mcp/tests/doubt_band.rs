//! The ask doubt band (`ask-judge.json`) through the MCP server: without a
//! working TypeSafe opt-in, or with a judge that cannot answer, the ask
//! answers exactly as it does without the band, and says why.
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
    call(&server, 1, "kmp_ingest", json!({"about": "project:band", "idempotency_key": "seed",
        "memory": {"dimensions": [{"id": "test", "kind": "task"}], "entries": [
            {"id": "project:band:entry:a", "kind": "observation", "text": "Issue #188 was deployed to the staging environment on Tuesday.",
             "coordinates": [{"dimension": "task", "scope_id": "test", "sequence": 1, "occurred_at": "2026-01-01T00:00:00Z"}]},
            {"id": "project:band:entry:b", "kind": "observation", "text": "Issue #188 pause resume preflight checks the journal.",
             "coordinates": [{"dimension": "task", "scope_id": "test", "sequence": 2, "occurred_at": "2026-01-01T00:00:00Z"}]}
        ], "relations": [], "evidence": []}})).await;
    server
}

const QUESTION: &str = "Which Kubernetes cluster was issue #188 deployed to?";

async fn ask(dir: &std::path::Path) -> Value {
    call(
        &seeded(dir).await,
        2,
        "kmp_ask",
        json!({"about": "project:band", "question": QUESTION}),
    )
    .await
}

#[tokio::test]
async fn the_band_without_the_typesafe_opt_in_warns_and_answers_as_today() {
    let plain = tempfile::tempdir().expect("dir");
    let baseline = ask(plain.path()).await;

    let opted = tempfile::tempdir().expect("dir");
    std::fs::write(opted.path().join("ask-judge.json"), "{}").expect("config");
    let banded = ask(opted.path()).await;

    let warnings = banded["warnings"].to_string();
    assert!(warnings.contains("doubt band disabled"), "{banded}");
    assert!(warnings.contains("typesafe.json"), "{banded}");
    assert_eq!(banded["answer"], baseline["answer"]);
    assert_eq!(banded["because"], baseline["because"]);
    assert_eq!(banded["answer_status"], baseline["answer_status"]);
}
