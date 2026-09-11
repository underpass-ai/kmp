use super::{packet_is_partial, tool_success_result};
use serde_json::json;

#[test]
fn completed_temporal_entries_do_not_hide_pending_proof_in_the_first_text() {
    let body = json!({"summary":"Returned 2 temporal entries.",
        "entries":[{"ref":"copy"},{"ref":"check"}],
        "selection":{"entries":2,"has_more":false},
        "page":{"has_more":true,"returned":25,"total":32,
            "sections":{"entries":{"remaining":0},"proof.path":{"remaining":7}}},
        "next_actions":[{"tool":"kmp_goto","arguments":{"continuation":"opaque-read"}}]});
    let native = serde_json::to_vec(&body).expect("body");
    let result = tool_success_result(body);
    assert_eq!(
        serde_json::to_vec(&result["structuredContent"]).expect("proof"),
        native
    );
    let text = result["content"][0]["text"].as_str().expect("first text");
    assert!(text.starts_with("READ_INCOMPLETE:"));
    assert!(text.ends_with("Returned 2 temporal entries."));
    assert_eq!(result["isError"], false);
}

#[test]
fn shortened_recall_core_remains_partial_even_when_expansion_is_finished() {
    let body = json!({"summary":"Selected state.","projection":{
        "core_text_shortened":true,"page":{"has_more":false},
        "next_action":{"tool":"kmp_wake","arguments":{"about":"p","budget":{"max_bytes":30000}}}}});
    let result = tool_success_result(body.clone());
    assert!(packet_is_partial(&body));
    assert_eq!(result["structuredContent"], body);
    assert!(
        result["content"][0]["text"]
            .as_str()
            .expect("notice")
            .starts_with("READ_INCOMPLETE:")
    );
}

#[test]
fn another_history_position_does_not_make_the_delivered_packet_partial() {
    let body = json!({"summary":"Returned 1 temporal entry.",
        "selection":{"has_more":true},"page":{"has_more":false},
        "next_actions":[{"tool":"kmp_rewind","arguments":{"about":"p","from":{"ref":"older"}}}]});
    let result = tool_success_result(body.clone());
    assert!(!packet_is_partial(&body));
    assert_eq!(result["structuredContent"], body);
    assert_eq!(result["content"][0]["text"], "Returned 1 temporal entry.");
}

#[test]
fn unknown_can_be_complete_or_pending_without_changing_the_native_answer() {
    for pending in [false, true] {
        let body = json!({"answer":"UNKNOWN","projection":{"core_text_shortened":false,
            "page":{"has_more":pending}}});
        let result = tool_success_result(body.clone());
        assert_eq!(result["structuredContent"], body);
        let text = result["content"][0]["text"].as_str().expect("text");
        assert_eq!(text.starts_with("READ_INCOMPLETE:"), pending);
        assert!(text.ends_with("UNKNOWN"));
    }
}

#[test]
fn writer_neighborhood_and_search_limits_do_not_invent_read_continuations() {
    for body in [
        json!({"summary":"Review neighbors.","status":"needs_review","neighborhood":{"partial":true}}),
        json!({"summary":"Bounded search.","seek":{"status":"partial"},"page":{"has_more":false}}),
    ] {
        assert!(!packet_is_partial(&body));
        let result = tool_success_result(body.clone());
        assert_eq!(result["structuredContent"], body);
        assert_eq!(result["content"][0]["text"], body["summary"]);
    }
}
