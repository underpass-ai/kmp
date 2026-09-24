//! The same returned response-page actions work over embedded, gRPC and HTTP.
use axum::Router;
use kmp_mcp::{GrpcKernelMcpBackend, KernelMcpServer, KernelMcpToolBackend};
use serde_json::json;

use super::{assert_error_code_parity, call_http_tool, call_tool, comparable};

pub(super) async fn check(
    direct: &GrpcKernelMcpBackend,
    stdio: &KernelMcpServer,
    http: &Router,
    embedded: &KernelMcpServer,
) {
    let mut arguments = json!({"move":"forward","about":"project:parity-live",
        "from":{"time":"2026-08-24T00:00:00Z"},"axis":"occurred",
        "include":{"evidence":true,"relations":true,"raw_refs":true},
        "limit":{"entries":10},"budget":{"max_bytes":50000}});
    let full = direct
        .call_tool("kmp_time", &arguments)
        .await
        .expect("full temporal packet");
    let full = &full["structuredContent"];
    assert_eq!(full["page"]["has_more"], false);
    assert_eq!(full["selection"]["entries"], 5);
    let pointers: Vec<_> = full["page"]["sections"]
        .as_object()
        .expect("sections")
        .keys()
        .map(|key| format!("/{}", key.replace('.', "/")))
        .collect();
    let mut reconstructed = full.clone();
    for pointer in &pointers {
        *reconstructed.pointer_mut(pointer).expect("section") = json!([]);
    }
    arguments["page"] = json!({"entries":2});
    let mut offset = 0;
    let mut first_continuation = None;
    loop {
        let expected = direct
            .call_tool("kmp_time", &arguments)
            .await
            .expect("temporal page");
        let native = call_tool(stdio, 100 + offset, "kmp_time", arguments.clone()).await;
        let http_page = call_http_tool(http, 100 + offset, "kmp_time", arguments.clone()).await;
        let local = call_tool(embedded, 100 + offset, "kmp_time", arguments.clone()).await;
        assert_eq!(comparable(&native["result"]), comparable(&expected));
        assert_eq!(comparable(&http_page["result"]), comparable(&expected));
        assert_eq!(comparable(&local["result"]), comparable(&expected));
        let content = &expected["structuredContent"];
        assert!(content.to_string().len() <= 50000);
        assert_eq!(content["page"]["offset"], offset);
        assert_eq!(content["selection"], full["selection"]);
        for pointer in &pointers {
            reconstructed
                .pointer_mut(pointer)
                .expect("section")
                .as_array_mut()
                .expect("items")
                .extend(
                    content
                        .pointer(pointer)
                        .expect("page section")
                        .as_array()
                        .expect("page items")
                        .iter()
                        .cloned(),
                );
        }
        let returned = content["page"]["returned"].as_u64().expect("returned");
        assert!(returned > 0 && returned <= 2);
        offset += returned;
        if content["page"]["has_more"] == false {
            break;
        }
        assert!(offset < 500, "must advance");
        let action = &content["next_actions"][0];
        assert_eq!(action["tool"], "kmp_time");
        assert_eq!(action["arguments"]["move"], "forward");
        arguments = action["arguments"].clone();
        first_continuation.get_or_insert_with(|| arguments.clone());
    }
    for pointer in &pointers {
        assert_eq!(
            reconstructed.pointer(pointer),
            full.pointer(pointer),
            "{pointer}"
        );
    }
    let mut changed = first_continuation.expect("fixture spans pages");
    changed["include"]["raw_refs"] = json!(false);
    assert_error_code_parity(
        direct, stdio, http, embedded, "kmp_time", changed, "conflict",
    )
    .await;
}
