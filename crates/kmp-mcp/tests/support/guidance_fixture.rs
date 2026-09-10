use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

pub const ABOUT: &str = "project:contextual-guidance";

pub async fn call(server: &KernelMcpServer, name: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":arguments}});
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    serde_json::from_str::<Value>(&response).expect("JSON")["result"].clone()
}

pub async fn open(server: &KernelMcpServer, key: &str) -> Value {
    let requests: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../plugins/kmp/guide/guide.requests.json"
    ))
    .expect("guide");
    for request in requests {
        assert_eq!(call(server, "kmp_ingest", request).await["isError"], false);
    }
    let result = call(server, "kmp_guide", json!({"registration_key":key})).await;
    assert_eq!(result["isError"], false, "{result}");
    result["structuredContent"].clone()
}

pub fn guidance(result: &Value) -> Value {
    let blocks: Vec<Value> = result["content"]
        .as_array()
        .expect("content")
        .iter()
        .filter_map(|item| serde_json::from_str::<Value>(item["text"].as_str()?).ok())
        .filter_map(|body| body.get("kmp_guidance").cloned())
        .collect();
    assert_eq!(blocks.len(), 1, "exactly one guidance block: {result}");
    assert!(result["structuredContent"].get("kmp_guidance").is_none());
    blocks[0].clone()
}

pub fn packet(context: &Value) -> Value {
    json!({"about":ABOUT,"context_id":context,"observed_at":"2026-09-09T10:00:00Z",
        "idempotency_key":"context:source:one", "labels":{"task":["context-check"]},
        "memories":[{"id":"source","kind":"observation","summary":"The route opens on Tuesday.",
            "evidence":"S1, the route notice, states that opening is on Tuesday."}]})
}
