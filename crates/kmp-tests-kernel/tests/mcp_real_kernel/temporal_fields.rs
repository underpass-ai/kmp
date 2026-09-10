//! All native transports execute the same reduced-entry expansion action.
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
    let args = json!({"about":"project:parity-live","axis":"occurred",
        "interval":{"start":"2026-08-25T00:01:00Z","end":"2026-08-25T00:03:00Z"},
        "fields":["coordinates"],"include":{"evidence":false,"relations":false},
        "limit":{"entries":10},"budget":{"max_bytes":50000}});
    let projected = direct
        .call_tool("kmp_forward", &args)
        .await
        .expect("projection");
    let mut full_args = args.clone();
    full_args
        .as_object_mut()
        .expect("arguments")
        .remove("fields");
    let full = direct
        .call_tool("kmp_forward", &full_args)
        .await
        .expect("full packet");
    let mut calls = vec![("kmp_forward", args, projected.clone())];
    for entry in projected["structuredContent"]["entries"]
        .as_array()
        .expect("entries")
    {
        assert!(entry.get("text").is_none() && entry.get("metadata").is_none());
        let action = &entry["detail_action"];
        let tool = action["tool"].as_str().expect("action tool");
        let detail = direct
            .call_tool(tool, &action["arguments"])
            .await
            .expect("expansion");
        assert_eq!(detail, full, "same selection, clock and proof");
        let expanded = detail["structuredContent"]["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .find(|item| item["ref"] == entry["ref"])
            .expect("selected entry");
        assert!(expanded["text"].is_string());
        assert_eq!(expanded["coordinates"], entry["coordinates"]);
        calls.push((tool, action["arguments"].clone(), detail));
    }
    assert_eq!(calls.len(), 3, "two reduced entries and their expansions");
    for (tool, arguments, expected) in calls {
        let native = call_tool(stdio, 800, tool, arguments.clone()).await;
        let remote = call_http_tool(http, 800, tool, arguments.clone()).await;
        let local = call_tool(embedded, 800, tool, arguments).await;
        assert_eq!(native["result"], expected, "{tool} stdio");
        assert_eq!(remote["result"], expected, "{tool} HTTP");
        assert_eq!(local["result"], expected, "{tool} embedded");
    }
}
