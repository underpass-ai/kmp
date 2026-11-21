//! `search.direction: bidirectional` is the single-destination trace. A call
//! that breaks it is refused at the boundary under the rule
//! `trace.bidirectional`, and the refusal names the argument this call sent
//! that the rule does not take, so an agent knows what to remove instead of
//! re-reading the whole rule.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:bidirectional-refusal";

async fn refusal(arguments: Value) -> Value {
    let server = KernelMcpServer::fixture();
    let wire = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
                "params":{"name":"kmp_trace","arguments":arguments}})
            .to_string(),
        )
        .await
        .expect("answered");
    let reply: Value = serde_json::from_str(&wire).expect("json");
    let content = reply["result"]["structuredContent"].clone();
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    assert_eq!(content["error"]["code"], "invalid_argument", "{content}");
    content
}

fn trace(change: impl FnOnce(&mut Value)) -> Value {
    let mut arguments = json!({"about": ABOUT, "from": format!("{ABOUT}:a"),
        "to": format!("{ABOUT}:b"), "search": {"direction": "bidirectional"}});
    change(&mut arguments);
    arguments
}

fn message(content: &Value) -> &str {
    content["error"]["message"].as_str().expect("message")
}

/// The refusal is the rule's: its id, its wording, what this call sent, the fix.
fn assert_under_the_rule(content: &Value, offending: &str) {
    let message = message(content);
    assert!(
        message.starts_with("Rule `trace.bidirectional`: "),
        "{message}"
    );
    assert!(
        message.contains(offending),
        "names `{offending}`: {message}"
    );
    assert!(message.contains(" Fix: "), "{message}");
    assert!(
        content["feedback"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|item| item["rule"] == "trace.bidirectional"),
        "feedback[].rule carries the rule: {content}"
    );
}

#[tokio::test]
async fn more_than_one_path_per_target_is_named() {
    let content = refusal(trace(|call| call["search"]["paths_per_target"] = json!(2))).await;
    assert_under_the_rule(&content, "search.paths_per_target");
}

#[tokio::test]
async fn several_destinations_are_named() {
    let content = refusal(trace(|call| {
        call["to"] = json!([format!("{ABOUT}:b"), format!("{ABOUT}:c")]);
    }))
    .await;
    assert_under_the_rule(&content, "kmp_trace.to");
}

#[tokio::test]
async fn a_time_selection_is_named() {
    let content = refusal(trace(|call| call["as_of"] = json!({"time": "2026-09-01T10:00:00Z"}))).await;
    assert_under_the_rule(&content, "as_of");
}

#[tokio::test]
async fn a_search_option_of_the_other_mode_is_named() {
    let content = refusal(trace(|call| call["search"]["same_labels"] = json!(["x"]))).await;
    assert!(message(&content).contains("same_labels"), "{content}");
    assert!(
        message(&content).starts_with("Rule `trace."),
        "a cross-field rule refuses it: {content}"
    );
}

#[tokio::test]
async fn a_bounded_bidirectional_trace_passes_the_rule() {
    let server = KernelMcpServer::fixture();
    let wire = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
                "params":{"name":"kmp_trace","arguments":trace(|call| {
                    call["search"] = json!({"direction":"bidirectional","max_nodes":64,"paths_per_target":1});
                })}})
            .to_string(),
        )
        .await
        .expect("answered");
    let reply: Value = serde_json::from_str(&wire).expect("json");
    let message = reply["result"]["structuredContent"]["error"]["message"]
        .as_str()
        .unwrap_or_default();
    assert!(!message.contains("trace.bidirectional"), "{reply}");
}

/// `to` belongs at the top level; inside `search` it is an unknown argument,
/// refused with the short standard message, not with the rule's wording.
#[tokio::test]
async fn to_inside_search_is_an_unknown_argument() {
    let content = refusal(trace(|call| call["search"]["to"] = Value::Null)).await;
    let message = message(&content);
    assert!(
        message.starts_with("`kmp_trace.search` has no argument `to`."),
        "{message}"
    );
    assert!(!message.contains("Rule `"), "{message}");
    assert!(
        content["feedback"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|item| item["code"] == "UNKNOWN_ARGUMENT" && item["field"] == "search.to"),
        "{content}"
    );
}
