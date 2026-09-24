//! A returned continuation is a handle the server resolves to the complete
//! call (#544 C3). Tests that check what the call preserves read it back
//! through the same resolution a transport performs before authorizing it.
#![allow(dead_code)]
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

pub fn bound_arguments(server: &KernelMcpServer, action: &Value) -> Value {
    server
        .resolve_read_request(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":action["tool"],"arguments":action["arguments"]}}))
        .expect("a returned action resolves")["params"]["arguments"]
        .clone()
}
