//! A changed selection must never be presented as the next page of an old read.
#[path = "support/relation_read_fixture.rs"]
mod fixture;
use fixture::{ABOUT, SECTIONS, call, seed};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn conflict(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}});
    let reply = server
        .handle_json_line(&request.to_string())
        .await
        .expect("reply");
    let result: Value = serde_json::from_str(&reply).expect("JSON");
    assert_eq!(result["result"]["isError"], true, "{result}");
    let content = &result["result"]["structuredContent"];
    assert_eq!(content["error"]["code"], "conflict", "{content}");
    assert_eq!(content["feedback"][0]["code"], "READ_SELECTION_CHANGED");
    assert!(content.get("trace").is_none() && content.get("facts").is_none());
    let action = content["feedback"][0]["action"].clone();
    assert_eq!(action["tool"], tool);
    assert!(action["arguments"].pointer("/page/cursor").is_none());
    call(server, tool, action["arguments"].clone()).await
}

#[tokio::test]
async fn relation_continuations_execute_without_reconstructing_selection() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    seed(&server).await;
    for (tool, mut query, sections) in [
        (
            "kmp_trace",
            json!({"about":ABOUT,"from":format!("{ABOUT}:observation:3"),
            "to":format!("{ABOUT}:observation:0"),"goal":"audit","budget":{"max_bytes":100000}}),
            &["trace"][..],
        ),
        (
            "kmp_relate",
            json!({"about":ABOUT,"axis":"observed",
            "interval":{"start":"2026-09-01T09:00:00Z","end":"2026-09-01T11:00:00Z"},
            "dimensions":{"selectors":[{"key":"component","op":"in","values":["component:export"]}]},
            "budget":{"max_bytes":100000}}),
            SECTIONS,
        ),
    ] {
        let full = call(&server, tool, query.clone()).await;
        let expected: Vec<_> = sections
            .iter()
            .flat_map(|s| full[*s].as_array().expect("section").clone())
            .collect();
        assert!(!expected.is_empty());
        query["page"] = json!({"entries":1});
        let mut actual = Vec::new();
        for _ in 0..expected.len() {
            let page = call(&server, tool, query.clone()).await;
            assert_eq!(page["page"]["offset"], actual.len());
            actual.extend(
                sections
                    .iter()
                    .flat_map(|s| page[*s].as_array().expect("section").clone()),
            );
            if page["page"]["has_more"] == false {
                assert_eq!(page["next_actions"], json!([]));
                break;
            }
            let action = &page["next_actions"][0];
            assert_eq!(action["tool"], tool);
            let mut previous = query.clone();
            previous["page"]["cursor"] = page["page"]["next_cursor"].clone();
            assert_eq!(action["arguments"], previous, "all bound arguments survive");
            query = action["arguments"].clone();
        }
        assert_eq!(actual, expected);
    }
}

#[tokio::test]
async fn relate_rejects_change_to_a_fact_that_was_not_on_the_first_page() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    seed(&server).await;
    let first = call(
        &server,
        "kmp_relate",
        json!({"about":ABOUT,"page":{"entries":1},"budget":{"max_bytes":100000}}),
    )
    .await;
    assert_eq!(first["facts"][0]["ref"], format!("{ABOUT}:observation:0"));
    call(&server, "kmp_ingest", json!({"about":ABOUT,"idempotency_key":"cursor:edit-hidden",
        "memory":{"dimensions":[],"relations":[],"entries":[{"id":format!("{ABOUT}:observation:3"),"kind":"observation",
            "text":"Later source content changed while reading earlier facts.","metadata":{"review":"changed"},
            "coordinates":[{"dimension":"component","scope_id":"component:export","observed_at":"2026-09-01T10:00:00Z"}]}]}})).await;
    let fresh = conflict(
        &server,
        "kmp_relate",
        first["next_actions"][0]["arguments"].clone(),
    )
    .await;
    assert_eq!(fresh["page"]["offset"], 0);
    assert_eq!(
        fresh["facts"][0], first["facts"][0],
        "the first page itself was unchanged"
    );
}

#[tokio::test]
async fn trace_rejects_new_endpoint_and_changed_proof_on_a_later_page() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    seed(&server).await;
    let args = json!({"about":ABOUT,"from":format!("{ABOUT}:observation:3"),
        "to":format!("{ABOUT}:observation:0"),"page":{"entries":1},"budget":{"max_bytes":100000}});
    let first = call(&server, "kmp_trace", args).await;
    let original = first["next_actions"][0]["arguments"].clone();
    let mut changed = original.clone();
    changed["from"] = json!(format!("{ABOUT}:observation:2"));
    let fresh = conflict(&server, "kmp_trace", changed).await;
    assert_eq!(fresh["trace"][0]["from"], format!("{ABOUT}:observation:2"));
    let inspected = call(
        &server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":format!("{ABOUT}:observation:1")}),
    )
    .await;
    call(&server,"kmp_ingest",json!({"about":ABOUT,"idempotency_key":"cursor:edit-later-proof",
        "memory":{"dimensions":[],"entries":[{"id":format!("{ABOUT}:observation:1"),"kind":"observation","text":inspected["object"]["text"],"coordinates":[{"dimension":"component","scope_id":"component:export","observed_at":"2026-09-01T10:00:00Z"}]}],"relations":[{"from":format!("{ABOUT}:observation:1"),"to":format!("{ABOUT}:observation:0"),
            "rel":"uses_background","class":"evidential","confidence":"high",
            "why":"The later proof annotation was corrected.","evidence":"Authored correction of the source citation."}]}})).await;
    let fresh = conflict(&server, "kmp_trace", original).await;
    assert_eq!(
        fresh["trace"], first["trace"],
        "unchanged first edge cannot hide a changed later edge"
    );
}
