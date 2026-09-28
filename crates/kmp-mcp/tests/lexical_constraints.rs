//! #850: `minLength` and `uniqueItems` left the advertised catalogue to save
//! tokens. The server still refuses what they forbade, and nothing commits.

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn rpc(server: &KernelMcpServer, method: &str, params: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
    let reply = server
        .handle_json_line(&request.to_string())
        .await
        .expect("reply");
    serde_json::from_str::<Value>(&reply).expect("JSON")["result"].clone()
}

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    rpc(
        server,
        "tools/call",
        json!({"name": tool, "arguments": arguments}),
    )
    .await
}

fn count(value: &Value, keyword: &str) -> usize {
    match value {
        Value::Object(object) => {
            usize::from(object.contains_key(keyword))
                + object.values().map(|v| count(v, keyword)).sum::<usize>()
        }
        Value::Array(items) => items.iter().map(|v| count(v, keyword)).sum(),
        _ => 0,
    }
}

#[tokio::test]
async fn the_catalogue_omits_the_constraints_the_server_enforces() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let listed = rpc(&server, "tools/list", json!({})).await;
    assert_eq!(count(&listed, "minLength"), 0);
    assert_eq!(count(&listed, "uniqueItems"), 0);
}

fn packet(label_values: Value) -> Value {
    json!({
        "about":"project:lexical", "actor":"writer",
        "idempotency_key":"lexical-1", "observed_at":"2026-09-01T09:00:00Z",
        "labels":{"component": label_values},
        "memories":[{"id":"source","kind":"observation","summary":"The cache failed.",
            "evidence":"Report R1: the cache failed."}]
    })
}

#[tokio::test]
async fn empty_and_repeated_values_are_refused_and_nothing_commits() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let before = store.export_bundle().await.expect("before");

    let refusals = [
        (
            "kmp_ingest",
            json!({"about":"project:lexical","idempotency_key":""}),
        ),
        ("kmp_write_memory", packet(json!([""]))),
        ("kmp_write_memory", packet(json!(["cache", "cache"]))),
        ("kmp_write_memory", {
            let mut packet = packet(json!(["cache"]));
            packet["idempotency_key"] = json!("");
            packet
        }),
    ];
    for (tool, arguments) in refusals {
        let refused = call(&server, tool, arguments.clone()).await;
        assert_eq!(refused["isError"], true, "{tool} {arguments}: {refused}");
    }

    assert_eq!(
        store.export_bundle().await.expect("after"),
        before,
        "a refused call must not write"
    );

    // Control: the same packet with one value per label is not refused.
    let accepted = call(&server, "kmp_write_memory", packet(json!(["cache"]))).await;
    assert_ne!(accepted["isError"], true, "{accepted}");
}
