//! Why a recall projection could not be produced: a rejected request, an
//! unusable continuation cursor, or a core too large for the ceiling.

use kmp_proto::v1beta1::{RecallCursorError as ProtoRecallCursorError, RecallCursorErrorReason};
use serde_json::Value;

use super::actions;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecallProjectionError {
    InvalidRequest(String),
    Cursor {
        reason: RecallCursorErrorReason,
        cursor: String,
        message: String,
        restart: Option<Value>,
    },
    CoreTooLarge,
}

impl RecallProjectionError {
    pub fn cursor_detail(&self) -> Option<ProtoRecallCursorError> {
        match self {
            Self::Cursor {
                reason,
                cursor,
                message,
                restart,
            } => Some(ProtoRecallCursorError {
                reason: *reason as i32,
                cursor: cursor.clone(),
                message: message.clone(),
                restart: restart.as_ref().map(actions::call_from_value),
            }),
            _ => None,
        }
    }
}

impl std::fmt::Display for RecallProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(message) => formatter.write_str(message),
            Self::Cursor { message, .. } => formatter.write_str(message),
            Self::CoreTooLarge => formatter.write_str(
                "recall projection byte budget is smaller than the stable citation core; \
                 raise budget.max_bytes",
            ),
        }
    }
}

impl std::error::Error for RecallProjectionError {}
