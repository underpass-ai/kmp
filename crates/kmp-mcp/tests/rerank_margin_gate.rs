//! The margin gate of Ask re-ranking (DESIGN L4 4c), through the tool: an
//! Ask the lexical ranking settles sends no judgement, and the same Ask with
//! the gate off does. The judge is an empty cassette in replay, so a request
//! that is sent shows up as `rerank unavailable` instead of reaching a
//! network. The check runs in a child of this test binary, which is given
//! the cassette environment.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn call(server: &KernelMcpServer, id: u64, name: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc":"2.0", "id":id, "method":"tools/call",
        "params":{"name":name, "arguments":arguments}})
    .to_string();
    let response = server.handle_json_line(&line).await.expect("MCP response");
    let value: Value = serde_json::from_str(&response).expect("JSON");
    assert!(value.get("error").is_none(), "{value}");
    value["result"]["structuredContent"].clone()
}

async fn seeded(dir: &std::path::Path, rerank: &str) -> KernelMcpServer {
    std::fs::write(
        dir.join("typesafe.json"),
        r#"{"endpoint":"https://api.typesafe.ai/v1/systemone","model":"jev-1.13.0","timeout_ms":20000}"#,
    )
    .expect("typesafe.json");
    std::fs::write(dir.join("rerank.json"), rerank).expect("rerank.json");
    let server = KernelMcpServer::embedded(dir).expect("server");
    call(&server, 1, "kmp_ingest", json!({"about": "project:gate", "idempotency_key": "seed",
        "memory": {"dimensions": [{"id": "test", "kind": "task"}], "entries": [
            {"id": "project:gate:entry:a", "kind": "observation", "text": "The car was fixed by Ana on Monday.",
             "coordinates": [{"dimension": "task", "scope_id": "test", "sequence": 1, "occurred_at": "2026-01-01T00:00:00Z"}]},
            {"id": "project:gate:entry:b", "kind": "observation", "text": "A technician repaired the automobile's brakes.",
             "coordinates": [{"dimension": "task", "scope_id": "test", "sequence": 2, "occurred_at": "2026-01-01T00:00:00Z"}]}
        ], "relations": [], "evidence": []}})).await;
    server
}

const CHILD_ENV: &str = "KMP_MARGIN_GATE_TEST_CHILD";
const NAME: &str = "a_settled_ask_sends_no_judgement_and_the_gate_can_be_turned_off";

#[test]
fn a_settled_ask_sends_no_judgement_and_the_gate_can_be_turned_off() {
    if std::env::var_os(CHILD_ENV).is_some() {
        tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(settled_and_open());
        return;
    }
    let cassette = tempfile::tempdir().expect("dir");
    let path = cassette.path().join("empty.cassette.json");
    std::fs::write(
        &path,
        r#"{"schema":"kmp.typesafe.cassette.v1","model":"jev-1.13.0","entries":{}}"#,
    )
    .expect("cassette");
    let status = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", NAME, "--nocapture"])
        .env(CHILD_ENV, "1")
        .env("KMP_TYPESAFE_CASSETTE", &path)
        .env("KMP_TYPESAFE_CASSETTE_MODE", "replay")
        .env("XDG_CONFIG_HOME", cassette.path())
        .env_remove("TYPESAFE_API_KEY")
        .status()
        .expect("child runs");
    assert!(status.success(), "the child check failed");
}

async fn settled_and_open() {
    let question = json!({"about": "project:gate", "question": "Who fixed the car?"});

    let gated = tempfile::tempdir().expect("dir");
    let settled = call(
        &seeded(gated.path(), r#"{"pool_size": 8}"#).await,
        2,
        "kmp_ask",
        question.clone(),
    )
    .await;
    let warnings = settled["warnings"].to_string();
    assert!(!warnings.contains("rerank unavailable"), "{settled}");
    assert!(!warnings.contains("rerank disabled"), "{settled}");
    assert!(!warnings.contains("evidence rerank by"), "{settled}");

    let open = tempfile::tempdir().expect("dir");
    let judged = call(
        &seeded(open.path(), r#"{"pool_size": 8, "margin_tenths": null}"#).await,
        2,
        "kmp_ask",
        question,
    )
    .await;
    assert!(
        judged["warnings"]
            .to_string()
            .contains("rerank unavailable"),
        "without the gate the ask is sent to the judge: {judged}"
    );
    assert_eq!(judged["answer"], settled["answer"]);
}
