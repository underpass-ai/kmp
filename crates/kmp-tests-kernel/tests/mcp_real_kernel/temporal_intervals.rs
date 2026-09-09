//! Interval selection and returned continuations have identical transport semantics.
use axum::Router;
use kmp_mcp::{GrpcKernelMcpBackend, KernelMcpServer, KernelMcpToolBackend};
use serde_json::json;

use super::{call_http_tool, call_tool};

pub(super) async fn check(
    direct: &GrpcKernelMcpBackend,
    stdio: &KernelMcpServer,
    http: &Router,
    embedded: &KernelMcpServer,
) {
    let interval = json!({"start":"2026-08-25T00:01:00Z","end":"2026-08-25T00:03:00Z"});
    for tool in ["kmp_forward", "kmp_rewind"] {
        let mut arguments = json!({"about":"project:parity-live", "axis":"occurred",
            "interval":interval, "limit":{"entries":1},"page":{"entries":2},
            "include":{"evidence":true,"relations":true,"raw_refs":true},
            "budget":{"max_bytes":50000}});
        let mut returned = Vec::new();
        let mut calls = 0;
        loop {
            let expected = direct
                .call_tool(tool, &arguments)
                .await
                .expect("interval read");
            let native = call_tool(stdio, 700 + calls, tool, arguments.clone()).await;
            let remote = call_http_tool(http, 700 + calls, tool, arguments.clone()).await;
            let local = call_tool(embedded, 700 + calls, tool, arguments.clone()).await;
            assert_eq!(native["result"], expected, "{tool} stdio");
            assert_eq!(remote["result"], expected, "{tool} HTTP");
            assert_eq!(local["result"], expected, "{tool} embedded");
            let content = &expected["structuredContent"];
            assert_eq!(content["temporal"]["interval"], interval);
            returned.extend(
                content["entries"]
                    .as_array()
                    .expect("entries")
                    .iter()
                    .map(|entry| entry["ref"].as_str().expect("ref").to_string()),
            );
            calls += 1;
            let actions = content["next_actions"].as_array().expect("actions");
            if actions.is_empty() {
                break;
            }
            assert!(calls < 50, "must advance");
            assert_eq!(actions[0]["tool"], tool);
            arguments = actions[0]["arguments"].clone();
            assert_eq!(arguments["interval"], interval);
            assert_eq!(arguments["axis"], "occurred");
        }
        returned.sort();
        assert_eq!(
            returned,
            [
                "project:parity-live:observation:parity-after",
                "project:parity-live:observation:parity-proof-1",
            ]
        );
        assert!(
            calls > 2,
            "proof pagination and history continuation were both exercised"
        );
    }
}
