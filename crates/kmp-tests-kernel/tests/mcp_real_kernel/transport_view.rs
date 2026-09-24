//! What transport parity compares (#544 C3). An MCP server hands back a
//! continuation as a short handle to the call it retained; the direct backend
//! has no server state and returns the complete call. Both continue the same
//! packet, so parity compares every other byte and masks only the action's
//! arguments and the byte count that includes them.
use serde_json::{Value, json};

fn is_continuation(action: &Value) -> bool {
    let arguments = &action["arguments"];
    arguments.get("continuation").is_some()
        || arguments
            .pointer("/page/cursor")
            .is_some_and(Value::is_string)
        || (action["tool"] == "kmp_write_memory" && arguments.get("review_token").is_some())
}

fn mask(action: &mut Value) {
    if is_continuation(action) {
        action["arguments"] = json!("<continuation>");
    }
}

/// A response with every continuation's arguments masked.
pub(super) fn comparable(response: &Value) -> Value {
    let mut response = response.clone();
    let body = if response.pointer("/structuredContent").is_some() {
        response.pointer_mut("/structuredContent")
    } else {
        response.pointer_mut("/result/structuredContent")
    };
    if let Some(body) = body {
        if let Some(action) = body.pointer_mut("/projection/next_action")
            && action.is_object()
        {
            mask(action);
            body["projection"]["budget"]["used_bytes"] = json!("<with its action>");
        }
        for action in body
            .get_mut("next_actions")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            mask(action);
        }
    }
    response
}
