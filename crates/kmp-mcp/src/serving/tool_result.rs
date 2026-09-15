//! The envelopes a tool answer is wrapped in before it leaves the server.
//!
//! One concept: how a success, an app-data success and an error are presented
//! to a host. What is *inside* `structuredContent` belongs to the mappers; this
//! module only decides the wrapper and the text fallback beside it.

use serde_json::{Value, json};

use crate::serving::ToolError;

/// Pending delivery inside this selected packet, not history outside it or
/// semantic completeness. Shared with optional guidance so both surfaces agree.
pub(crate) fn packet_is_partial(body: &Value) -> bool {
    body.pointer("/page/has_more") == Some(&Value::Bool(true))
        || body.pointer("/projection/page/has_more") == Some(&Value::Bool(true))
        || body.pointer("/projection/core_text_shortened") == Some(&Value::Bool(true))
}

pub(crate) fn tool_success_result(structured_content: Value) -> Value {
    // `structuredContent` is the canonical response. Repeating the entire
    // pretty-printed JSON in the text block doubled every tool result and was
    // enough to overflow hosts even after the structured packet was budgeted.
    let mut text = structured_content
        .get("summary")
        .and_then(Value::as_str)
        .or_else(|| structured_content.get("answer").and_then(Value::as_str))
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            serde_json::to_string(&structured_content)
                .expect("fixture JSON should serialize as compact text")
        });
    if packet_is_partial(&structured_content) {
        text.insert_str(0, "READ_INCOMPLETE: finish the selected packet using the returned read action before concluding. If unavailable, keep the result partial.\n");
    }
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

#[cfg(test)]
#[path = "tool_result_read_tests.rs"]
mod read_tests;

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
            "ui": {"resourceUri": crate::contract::CHRONOLOOM_APP_URI},
            "kmp/modelContext": "receipt-only"
        }
    })
}

pub(crate) fn tool_error_result(tool: &str, arguments: &Value, error: &ToolError) -> Value {
    let guide_error = (tool == "kmp_inspect")
        .then(|| super::guide_repair::GuideRepair::for_inspect(arguments, error))
        .flatten();
    let error = guide_error.as_ref().unwrap_or(error);
    let mut result = json!({
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
    });
    if tool == "kmp_write_memory" {
        result["structuredContent"]["status"] = json!(if error.code
            == crate::serving::ToolErrorCode::InvalidArgument
        {
            "rejected"
        } else {
            // A transport/backend error can follow persistence. Do not
            // promise that nothing was written; retry the same logical key.
            "unconfirmed"
        });
    }
    if !error.feedback.is_empty() {
        result["structuredContent"]["feedback"] = json!(error.feedback);
    }
    if let Some(help) = super::tool_error_help::ToolErrorHelp::for_call(tool, arguments, error) {
        // Text-only hosts must receive the same callable lessons. Do not copy
        // guide bodies here or replace an existing repair/restart action.
        result["content"][0]["text"] = json!(format!("{}\nUsage help: {}", error.message, help));
        result["structuredContent"]["help"] = help;
    }
    result
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

        let error = tool_error_result("kmp_ask", &json!({}), &ToolError::backend("no evidence"));
        assert_eq!(error["isError"], true);
        assert_eq!(error["content"][0]["text"], "no evidence");
        assert_eq!(error["structuredContent"]["error"]["code"], "backend_error");

        let uncertain = tool_error_result(
            "kmp_write_memory",
            &json!({}),
            &ToolError::backend("lost reply"),
        );
        assert_eq!(uncertain["structuredContent"]["status"], "unconfirmed");
        assert!(uncertain["structuredContent"].get("accepted").is_none());

        let missing = tool_error_result(
            "kmp_inspect",
            &json!({}),
            &ToolError::not_found("node `question:missing` not found"),
        );
        assert_eq!(missing["structuredContent"]["error"]["code"], "not_found");
    }
}
