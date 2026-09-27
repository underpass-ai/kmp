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

/// An incremental recall continuation has no summary or answer of its own:
/// those travel with the core on the first page. Its text stays constant
/// instead of copying the new items a second time.
fn recall_continuation_text(body: &Value) -> Option<&'static str> {
    (body.pointer("/projection/core_reused") == Some(&Value::Bool(true))).then_some(
        "Recall continuation: new expansion items only; combine them with the first page's core and earlier pages.",
    )
}

pub(crate) fn tool_success_result(structured_content: Value) -> Value {
    // `structuredContent` is the canonical response. Repeating the entire
    // pretty-printed JSON in the text block doubled every tool result and was
    // enough to overflow hosts even after the structured packet was budgeted.
    let mut text = structured_content
        .get("summary")
        .and_then(Value::as_str)
        .or_else(|| structured_content.get("answer").and_then(Value::as_str))
        .or_else(|| recall_continuation_text(&structured_content))
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
    // A write refusal names the fields to repair in the text too: a host that
    // reads only text must be able to repair the packet in one pass.
    let text = if tool == "kmp_write_memory" {
        refused_fields_text(error)
    } else {
        error.message.clone()
    };
    let mut result = json!({
        "content": [
            {
                "type": "text",
                "text": text
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
        result["content"][0]["text"] = json!(format!("{text}\nUsage help: {help}"));
        result["structuredContent"]["help"] = help;
    }
    result
}

/// The refusal as a text-only host reads it: the message, then every refused
/// field with its reason, one per line. A single failure whose reason is the
/// message is prefixed with its field instead of being repeated.
fn refused_fields_text(error: &ToolError) -> String {
    let refused = error
        .feedback
        .iter()
        .filter(|item| item["severity"] == "error")
        .filter_map(|item| Some((item["field"].as_str()?, item["reason"].as_str()?)))
        .collect::<Vec<_>>();
    match refused.as_slice() {
        [] => error.message.clone(),
        [(field, reason)] if *reason == error.message => {
            if field.is_empty() {
                error.message.clone()
            } else {
                format!("{field}: {reason}")
            }
        }
        refused => {
            let mut text = error.message.clone();
            for (field, reason) in refused {
                let field = if field.is_empty() { "request" } else { field };
                text.push_str(&format!("\n- {field}: {reason}"));
            }
            text
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_write_refusal_lists_every_refused_field_in_its_text() {
        let error = ToolError::invalid_argument("2 validation failures in 2 records")
            .with_feedback(json!({"code":"INVALID_LABELS","severity":"error","field":"memories[0].labels","reason":"labels must declare"}))
            .with_feedback(json!({"code":"INVALID_KIND","severity":"error","field":"memories[1].kind","reason":"kind is invalid"}))
            .with_feedback(json!({"code":"RELATION_CONTEXT_UNVERIFIED","severity":"warning","field":"memories[1].connect_to[0]","reason":"a warning"}));

        let result = tool_error_result("kmp_write_memory", &json!({}), &error);
        let text = result["content"][0]["text"].as_str().expect("text");

        assert!(
            text.starts_with("2 validation failures in 2 records\n"),
            "{text}"
        );
        assert!(
            text.contains("\n- memories[0].labels: labels must declare"),
            "{text}"
        );
        assert!(
            text.contains("\n- memories[1].kind: kind is invalid"),
            "{text}"
        );
        assert!(!text.contains("a warning"), "{text}");
        assert_eq!(
            result["structuredContent"]["error"]["message"],
            "2 validation failures in 2 records"
        );
    }

    #[test]
    fn a_single_write_refusal_names_its_field_once() {
        let error = ToolError::invalid_argument("kind is invalid").with_feedback(
            json!({"code":"INVALID_KIND","severity":"error","field":"memories[0].kind","reason":"kind is invalid"}),
        );
        let result = tool_error_result("kmp_write_memory", &json!({}), &error);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .expect("text")
                .starts_with("memories[0].kind: kind is invalid")
        );

        let whole = ToolError::invalid_argument("provide exactly one").with_feedback(
            json!({"code":"WRITE_OPERATION_REQUIRED","severity":"error","field":"","reason":"provide exactly one"}),
        );
        let result = tool_error_result("kmp_write_memory", &json!({}), &whole);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .expect("text")
                .starts_with("provide exactly one")
        );

        // Other tools keep the message as their text.
        let result = tool_error_result("kmp_relabel", &json!({}), &error);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .expect("text")
                .starts_with("kind is invalid")
        );
    }

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
