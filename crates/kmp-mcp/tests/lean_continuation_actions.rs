//! Continuation actions are minimal (#544 C3): the server retains the call it
//! proposes and hands back a short handle, so a page never restates the
//! request. A host that executes the action verbatim still completes the
//! packet; a stale selection is still rejected by the cursor it retained.
#[path = "support/write_neighborhood_fixture.rs"]
mod neighborhood;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:lean-actions";

async fn raw(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}});
    let line = server
        .handle_json_line(&request.to_string())
        .await
        .expect("reply");
    serde_json::from_str::<Value>(&line).expect("JSON")["result"].clone()
}

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let result = raw(server, tool, arguments).await;
    assert_ne!(result["isError"], true, "{result}");
    result["structuredContent"].clone()
}

async fn seed(server: &KernelMcpServer, key: &str, count: usize) {
    let memories = (0..count)
        .map(|index| {
            json!({"id":format!("m{index}"),"kind":"observation",
                "summary":format!("Observation {index} about the export pipeline."),
                "evidence":format!("Log {index} records the export pipeline step {index}. ").repeat(6),
                "occurred_at":format!("2026-09-01T{:02}:00:00Z", 1 + index)})
        })
        .collect::<Vec<_>>();
    let written = call(
        server,
        "kmp_write_memory",
        json!({"about":ABOUT,"actor":"lean","observed_at":"2026-09-01T23:00:00Z",
            "idempotency_key":key,"labels":{"component":["export"]},"memories":memories}),
    )
    .await;
    assert!(
        matches!(written["status"].as_str(), Some("committed")),
        "{written}"
    );
}

fn handle(action: &Value) -> &str {
    let arguments = action["arguments"].as_object().expect("arguments");
    assert_eq!(
        arguments.len(),
        1,
        "a continuation restates nothing: {action}"
    );
    let handle = arguments["continuation"].as_str().expect("handle");
    assert!(
        handle.starts_with("read_") && handle.len() == 37,
        "{handle}"
    );
    handle
}

fn resolved(server: &KernelMcpServer, action: &Value) -> Value {
    server
        .resolve_read_request(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":action["tool"],"arguments":action["arguments"]}}))
        .expect("resolve")["params"]["arguments"]
        .clone()
}

/// Time and Inspect pages report per section only what is left to derive:
/// no zero counter, no empty section, no `total` (earlier pages plus this
/// page's `returned_on_page` plus `remaining`).
fn assert_lean_page_sections(body: &Value) {
    let sections = body["page"]["sections"].as_object().expect("sections");
    for (name, counts) in sections {
        let counts = counts.as_object().expect("counts");
        assert!(!counts.is_empty(), "{name} is empty: {body}");
        for (key, value) in counts {
            assert!(
                matches!(key.as_str(), "returned_on_page" | "remaining"),
                "{name}.{key} is derivable: {body}"
            );
            assert!(value.as_u64() > Some(0), "{name}.{key} is zero: {body}");
        }
    }
    assert!(body["page"].get("omitted").is_none(), "{body}");
}

fn wake_arguments() -> Value {
    json!({"about":ABOUT,"intent":"review the export",
        "dimensions":{"selectors":[{"key":"component","op":"in","values":["export"]}]},
        "budget":{"max_bytes":4000,"detail":"full","depth":3}})
}

#[tokio::test]
async fn recall_continuations_are_handles_that_complete_the_packet() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    seed(&server, "lean:recall", 8).await;
    for tool in ["kmp_wake", "kmp_ask"] {
        let mut initial = wake_arguments();
        if tool == "kmp_ask" {
            initial["question"] = json!("What does the export pipeline log record?");
            initial.as_object_mut().expect("args").remove("intent");
            // Ask's core is larger; start where it fits, so pages continue.
            initial["budget"]["max_bytes"] = json!(9000);
        }
        let mut whole_args = initial.clone();
        whole_args["budget"]["max_bytes"] = json!(1_000_000);
        let whole = call(&server, tool, whole_args).await;
        assert_eq!(whole["projection"]["page"]["has_more"], false);

        let mut page = call(&server, tool, initial.clone()).await;
        let mut pages = 1;
        let mut path = page["proof"]["path"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        while let Some(action) = page["projection"]["next_action"].as_object().cloned() {
            let action = Value::Object(action);
            assert_eq!(action["tool"], tool);
            let _ = handle(&action);
            // The handle stands for the full call, cursor included.
            let bound = resolved(&server, &action);
            for key in ["about", "intent", "question"] {
                assert_eq!(bound[key], initial[key], "{tool}: {key}");
            }
            assert_eq!(
                bound["dimensions"]["selectors"], initial["dimensions"]["selectors"],
                "{tool}"
            );
            assert_eq!(
                bound["page"]["cursor"], page["projection"]["page"]["next_cursor"],
                "{tool}"
            );
            page = call(&server, tool, action["arguments"].clone()).await;
            assert_eq!(page["projection"]["core_reused"], true, "{page}");
            path.extend(
                page["proof"]["path"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            );
            pages += 1;
            assert!(pages < 40, "{tool} continuations must progress");
        }
        assert!(pages > 1, "{tool}: the fixture must page");
        assert_eq!(json!(path), whole["proof"]["path"], "{tool}");
    }
}

#[tokio::test]
async fn a_handle_accepts_repeat_core_and_nothing_else() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    seed(&server, "lean:rehydrate", 8).await;
    let first = call(&server, "kmp_wake", wake_arguments()).await;
    let action = first["projection"]["next_action"].clone();
    let id = handle(&action).to_string();

    let rehydrated = call(
        &server,
        "kmp_wake",
        json!({"continuation":id,"page":{"repeat_core":true}}),
    )
    .await;
    assert!(rehydrated["projection"].get("core_reused").is_none());
    assert_eq!(rehydrated["summary"], first["summary"]);
    if let Some(next) = rehydrated["projection"]["next_action"].as_object() {
        let next = Value::Object(next.clone());
        let bound = resolved(&server, &next);
        assert!(bound["page"].get("repeat_core").is_none(), "{bound}");
    }

    for extra in [
        json!({"continuation":id,"about":"project:other"}),
        json!({"continuation":id,"page":{"cursor":"kmp1:1:00"}}),
        json!({"continuation":id,"page":{"repeat_core":"yes"}}),
        json!({"continuation":id,"budget":{"max_bytes":4096}}),
    ] {
        let error = raw(&server, "kmp_wake", extra.clone()).await;
        assert_eq!(error["isError"], true, "{extra}: {error}");
        assert_eq!(
            error["structuredContent"]["error"]["code"], "invalid_argument",
            "{extra}"
        );
    }
    let unknown = raw(
        &server,
        "kmp_wake",
        json!({"continuation":"read_00000000000000000000000000000000"}),
    )
    .await;
    assert_eq!(unknown["isError"], true);
    assert_eq!(
        unknown["structuredContent"]["feedback"][0]["code"],
        "CONTINUATION_UNAVAILABLE"
    );
    // A handle belongs to the verb that returned it.
    let crossed = raw(&server, "kmp_ask", json!({"continuation":id})).await;
    assert_eq!(crossed["isError"], true);
}

#[tokio::test]
async fn a_handle_to_a_changed_selection_is_rejected_by_its_cursor() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    seed(&server, "lean:stale", 8).await;
    let first = call(&server, "kmp_wake", wake_arguments()).await;
    let action = first["projection"]["next_action"].clone();
    handle(&action);
    seed(&server, "lean:stale:later", 2).await;
    let stale = raw(&server, "kmp_wake", action["arguments"].clone()).await;
    assert_eq!(stale["isError"], true, "{stale}");
    assert_eq!(stale["structuredContent"]["error"]["code"], "conflict");
    // The restart it proposes is a fresh read, not another handle.
    let restart = &stale["structuredContent"]["feedback"][0]["action"];
    assert_eq!(restart["arguments"]["about"], ABOUT, "{stale}");
    call(&server, "kmp_wake", restart["arguments"].clone()).await;
}

#[tokio::test]
async fn inspect_and_time_continuations_are_handles() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    seed(&server, "lean:time", 8).await;
    let entries = call(
        &server,
        "kmp_time",
        json!({"about":ABOUT,"move":"rewind","axis":"occurred","from":{"time":"2026-09-02T00:00:00Z"},
            "include":{"evidence":true},"budget":{"max_bytes":1500}}),
    )
    .await;
    assert_eq!(entries["page"]["has_more"], true, "{entries}");
    assert_lean_page_sections(&entries);
    let action = entries["next_actions"][0].clone();
    assert_eq!(action["tool"], "kmp_time");
    handle(&action);
    // The handle keeps the move the server needs to continue.
    assert_eq!(resolved(&server, &action)["move"], "rewind");
    let next = call(&server, "kmp_time", action["arguments"].clone()).await;
    assert!(next["page"]["returned"].as_u64() > Some(0), "{next}");
    assert_lean_page_sections(&next);

    let reference = next["entries"][0]["ref"].clone();
    let inspected = call(
        &server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":reference,"include":{"raw":true},"budget":{"max_bytes":512}}),
    )
    .await;
    assert_lean_page_sections(&inspected);
    if inspected["page"]["has_more"] == true {
        let action = inspected["next_actions"][0].clone();
        handle(&action);
        call(&server, "kmp_inspect", action["arguments"].clone()).await;
    }
}

#[tokio::test]
async fn a_write_review_resumes_from_a_handle() {
    let dir = neighborhood::scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let pending = neighborhood::call(&server, "kmp_write_memory", neighborhood::packet()).await;
    assert_eq!(pending["status"], "needs_review", "{pending}");
    let action = pending["next_actions"][0].clone();
    assert_eq!(action["tool"], "kmp_write_memory");
    handle(&action);
    let bound = resolved(&server, &action);
    assert_eq!(bound["review_token"], pending["neighborhood"]["token"]);
    assert_eq!(bound["memories"], neighborhood::packet()["memories"]);
    let committed = neighborhood::resume(&server, &pending).await;
    assert_eq!(committed["status"], "committed", "{committed}");
}
