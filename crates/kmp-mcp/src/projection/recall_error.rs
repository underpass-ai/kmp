//! Preserve typed recall cursor failures and their complete recovery call.
use kmp_proto::v1beta1::{RecallCursorError, RecallCursorErrorReason};
use kmp_proto_mapping::v1beta1::recall_projection::RecallProjectionError;
use serde_json::{Value, json};

use crate::serving::ToolError;

pub(crate) fn projection(error: RecallProjectionError) -> ToolError {
    match error.cursor_detail() {
        Some(detail) => cursor(detail),
        None => ToolError::invalid_argument(error.to_string()),
    }
}

pub(crate) fn cursor(detail: RecallCursorError) -> ToolError {
    let changed = detail.reason == RecallCursorErrorReason::SelectionChanged as i32;
    let error = if changed {
        ToolError::conflict(&detail.message)
    } else {
        ToolError::invalid_argument(&detail.message)
    };
    let action = detail.restart.and_then(|call| {
        serde_json::from_str::<Value>(&call.arguments_json)
            .ok()
            .map(|arguments| json!({"tool":call.tool,"arguments":arguments}))
    });
    error.with_feedback(json!({
        "code":if changed { "READ_SELECTION_CHANGED" } else { "INVALID_READ_CURSOR" },
        "field":"page.cursor", "message":detail.message,
        "action":action
    }))
}
