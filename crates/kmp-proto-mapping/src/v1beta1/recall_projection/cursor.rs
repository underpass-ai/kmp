//! The recall continuation cursor: the hash that binds it to one selection
//! and the opaque token a caller returns to read the next page.

use kmp_proto::v1beta1::RecallCursorErrorReason;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::metadata::PROJECTION_CONTRACT;
use super::plan::{ProjectionItem, ProjectionPlan};
use super::projection_error::RecallProjectionError;

const CURSOR_VERSION: &str = "kmp1";

pub(super) fn selection_hash(
    arguments: &Value,
    plan: &ProjectionPlan,
    eligible: &[ProjectionItem],
) -> String {
    let mut bound_arguments = arguments.clone();
    if let Some(arguments) = bound_arguments.as_object_mut() {
        arguments.remove("page");
        if let Some(budget) = arguments.get_mut("budget").and_then(Value::as_object_mut) {
            budget.remove("tokens");
            budget.remove("max_bytes");
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(PROJECTION_CONTRACT.as_bytes());
    hasher.update(b"\0");
    hasher.update(
        serde_json::to_vec(&bound_arguments).expect("projection arguments should serialize"),
    );
    hasher.update(b"\0");
    // Bind the cursor to the canonical projection plan, not the raw response
    // serialization returned by a graph adapter. Storage is allowed to return
    // equal-ranked rows in any order; ProjectionPlan gives expansion items a
    // total semantic order before this identity is computed. Hash the stable
    // core plus that ordered selection so identical snapshots produce the
    // same cursor across requests and transports, while any eligible semantic
    // change still invalidates it.
    hasher.update(
        serde_json::to_vec(&plan.core).expect("projection core should serialize canonically"),
    );
    for item in eligible {
        hasher.update(b"\0");
        hasher.update(item.section.name().as_bytes());
        hasher.update(b"\0");
        hasher.update(item.stable_key.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

pub(super) fn parse_cursor(
    cursor: Option<&str>,
    selection_hash: &str,
    total: usize,
) -> Result<usize, RecallProjectionError> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let mut parts = cursor.split(':');
    let version = parts.next();
    let offset = parts.next();
    let hash = parts.next();
    if version != Some(CURSOR_VERSION)
        || offset.is_none()
        || hash.is_none()
        || parts.next().is_some()
    {
        return Err(cursor_error(
            RecallCursorErrorReason::Malformed,
            cursor,
            "invalid page.cursor: malformed recall continuation",
        ));
    }
    if hash != Some(selection_hash) {
        return Err(cursor_error(
            RecallCursorErrorReason::SelectionChanged,
            cursor,
            "invalid page.cursor: it does not match this recall selection",
        ));
    }
    let offset = offset
        .and_then(|offset| offset.parse::<usize>().ok())
        .ok_or_else(|| {
            cursor_error(
                RecallCursorErrorReason::Malformed,
                cursor,
                "invalid page.cursor: malformed recall continuation offset",
            )
        })?;
    if offset > total {
        return Err(cursor_error(
            RecallCursorErrorReason::OffsetOutOfRange,
            cursor,
            "invalid page.cursor: continuation offset is out of range",
        ));
    }
    Ok(offset)
}

fn cursor_error(
    reason: RecallCursorErrorReason,
    cursor: &str,
    message: &str,
) -> RecallProjectionError {
    RecallProjectionError::Cursor {
        reason,
        cursor: cursor.to_string(),
        message: message.to_string(),
        restart: None,
    }
}

pub(super) fn make_cursor(offset: usize, selection_hash: &str) -> String {
    format!("{CURSOR_VERSION}:{offset}:{selection_hash}")
}
