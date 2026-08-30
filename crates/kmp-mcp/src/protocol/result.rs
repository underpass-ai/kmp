//! The envelopes a tool answer is wrapped in before it leaves the server.
//!
//! One concept: how a success, an app-data success and an error are presented
//! to a host. What is *inside* `structuredContent` belongs to the mappers; this
//! module only decides the wrapper and the text fallback beside it.

use serde_json::{Value, json};

use crate::tool_error::ToolError;

pub(crate) fn tool_success_result(structured_content: Value) -> Value {
    // `structuredContent` is the canonical response. Repeating the entire
    // pretty-printed JSON in the text block doubled every tool result and was
    // enough to overflow hosts even after the structured packet was budgeted.
    let text = structured_content
        .get("summary")
        .and_then(Value::as_str)
        .or_else(|| structured_content.get("answer").and_then(Value::as_str))
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            serde_json::to_string(&structured_content)
                .expect("fixture JSON should serialize as compact text")
        });
    json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ],
        "structuredContent": structured_content,
        "isError": false
    })
}

/// UI data remains in `structuredContent`, which MCP Apps delivers to the
/// sandbox without copying it into model context. The text fallback stays a
/// constant-size receipt for hosts that expose tool logs to a model.
pub(crate) fn app_data_success_result(structured_content: Value) -> Value {
    let returned = structured_content["page"]["returned"]
        .as_u64()
        .unwrap_or_default();
    json!({
        "content": [{
            "type": "text",
            "text": format!("ChronoLoom visual data chunk ready ({returned} detailed entries).")
        }],
        "structuredContent": structured_content,
        "_meta": {
            "ui": {"resourceUri": super::CHRONOLOOM_APP_URI},
            "kmp/modelContext": "receipt-only"
        }
    })
}

pub(crate) fn tool_error_result(error: &ToolError) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": error.message
            }
        ],
        "structuredContent": {
            "error": {
                "code": error.code.as_str(),
                "message": error.message
            }
        },
        "isError": true
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_results_are_mcp_content_blocks() {
        let success = tool_success_result(json!({"answer": "Austin"}));
        assert_eq!(success["isError"], false);
        assert_eq!(success["structuredContent"]["answer"], "Austin");
        assert!(
            success["content"][0]["text"]
                .as_str()
                .expect("content text should be present")
                .contains("Austin")
        );

        let error = tool_error_result(&ToolError::backend("no evidence"));
        assert_eq!(error["isError"], true);
        assert_eq!(error["content"][0]["text"], "no evidence");
        assert_eq!(error["structuredContent"]["error"]["code"], "backend_error");

        let missing = tool_error_result(&ToolError::not_found("node `question:missing` not found"));
        assert_eq!(missing["structuredContent"]["error"]["code"], "not_found");
    }
}
