//! One small embedded store about a route opening, for the read tests that
//! page and continue over real memory.
#![allow(dead_code)]
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

pub const ABOUT: &str = "project:route-reads";

pub async fn call(server: &KernelMcpServer, name: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":arguments}});
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    serde_json::from_str::<Value>(&response).expect("JSON")["result"].clone()
}

pub fn packet() -> Value {
    json!({"about":ABOUT,"actor":"route-reader","observed_at":"2026-09-09T10:00:00Z",
        "idempotency_key":"route:source:one", "labels":{"task":["route-check"]},
        "memories":[{"id":"source","kind":"observation","summary":"The route opens on Tuesday.",
            "evidence":"S1, the route notice, states that opening is on Tuesday."}]})
}
