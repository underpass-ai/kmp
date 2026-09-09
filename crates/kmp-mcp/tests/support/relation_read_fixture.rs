use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};
pub const ABOUT: &str = "project:relation-budget";
pub const SECTIONS: &[&str] = &["facts", "declared", "coordinate", "tensions", "proposed"];

pub async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}});
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("valid native fixture");
    let response: Value = serde_json::from_str(&response).expect("valid native fixture");
    assert!(response.get("error").is_none(), "{response}");
    assert_ne!(response["result"]["isError"], true, "{response}");
    response["result"]["structuredContent"].clone()
}

pub async fn seed(server: &KernelMcpServer) {
    let entries: Vec<_> = (0..4).map(|i| json!({
        "id":format!("{ABOUT}:observation:{i}"), "kind":"observation",
        "text":format!("Source {i}: {}", "The authored log retains its complete evidence. ".repeat(30)),
        "coordinates":[{"dimension":"component","scope_id":"component:export",
            "observed_at":"2026-09-01T10:00:00Z"}]
    })).collect();
    let relations: Vec<_> = (1..4).map(|i| json!({
        "from":format!("{ABOUT}:observation:{i}"), "to":format!("{ABOUT}:observation:{}",i-1),
        "rel":"uses_background", "class":"evidential", "confidence":"high",
        "why":format!("Source {i} uses the preceding log as background. {}", "Retain this full rationale. ".repeat(30)),
        "evidence":format!("Fictional source {i} explicitly cites its predecessor. {}", "The original source text remains verbatim. ".repeat(30))
    })).collect();
    call(server,"kmp_ingest",json!({"about":ABOUT,"idempotency_key":"relation-budget:seed",
        "memory":{"dimensions":[{"id":"component:export","kind":"component"}],"entries":entries,"relations":relations}})).await;
}
