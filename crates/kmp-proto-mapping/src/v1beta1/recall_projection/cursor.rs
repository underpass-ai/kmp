//! The recall continuation cursor: the hash that binds it to one selection
//! and the opaque token a caller returns to read the next page.
//!
//! `kmp2` (P14) carries the resume state of a ranked reading: the offset,
//! a digest of the last item the previous page returned (the boundary the
//! next page resumes after) and the selection hash. A page resumes only
//! after the exact item it stopped at; anything else is a changed selection
//! and never a silently shifted page. A `kmp1` cursor, from the contract
//! before it, is refused with the same reason as a changed selection and a
//! message that says to restart the reading.

use std::borrow::Borrow;

use kmp_proto::v1beta1::RecallCursorErrorReason;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::metadata::PROJECTION_CONTRACT;
use super::plan::{ProjectionItem, ProjectionPlan};
use super::projection_error::RecallProjectionError;

const CURSOR_VERSION: &str = "kmp2";
/// The cursor of the contract before P14: an offset and a selection hash,
/// with no boundary.
const RETIRED_VERSION: &str = "kmp1";
/// Hex digits of the boundary digest.
const BOUNDARY_HEX: usize = 16;

pub(super) fn selection_hash<T>(arguments: &Value, plan: &ProjectionPlan, eligible: &[T]) -> String
where
    T: Borrow<ProjectionItem>,
{
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
        let item = item.borrow();
        hasher.update(b"\0");
        hasher.update(item.section.name().as_bytes());
        hasher.update(b"\0");
        hasher.update(item.stable_key.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// The digest of the item a page ended on: what the next page resumes after.
pub(super) fn boundary_of(item: &ProjectionItem) -> String {
    let mut hasher = Sha256::new();
    hasher.update(item.section.name().as_bytes());
    hasher.update(b"\0");
    hasher.update(item.stable_key.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    digest[..BOUNDARY_HEX].to_string()
}

/// The boundary of a reading that returned nothing yet.
pub(super) fn start_boundary() -> String {
    "0".repeat(BOUNDARY_HEX)
}

/// The resume boundary after the first `offset` eligible items.
pub(super) fn boundary_after<T>(eligible: &[T], offset: usize) -> String
where
    T: Borrow<ProjectionItem>,
{
    match offset.checked_sub(1).and_then(|last| eligible.get(last)) {
        Some(item) => boundary_of(item.borrow()),
        None => start_boundary(),
    }
}

/// The widest cursor a page can carry, for sizing a page before it is cut.
pub(super) fn widest_cursor() -> String {
    make_cursor(usize::MAX, &"f".repeat(BOUNDARY_HEX), &"f".repeat(64))
}

pub(super) fn parse_cursor<T>(
    cursor: Option<&str>,
    selection_hash: &str,
    eligible: &[T],
) -> Result<usize, RecallProjectionError>
where
    T: Borrow<ProjectionItem>,
{
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let mut parts = cursor.split(':');
    let version = parts.next();
    if version == Some(RETIRED_VERSION) {
        return Err(cursor_error(
            RecallCursorErrorReason::SelectionChanged,
            cursor,
            "invalid page.cursor: it is a kmp1 continuation from the recall contract before \
             kmp2, which cannot resume this reading; restart the recall without page.cursor",
        ));
    }
    let offset = parts.next();
    let boundary = parts.next();
    let hash = parts.next();
    if version != Some(CURSOR_VERSION)
        || offset.is_none()
        || boundary.is_none_or(|boundary| boundary.len() != BOUNDARY_HEX)
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
    if offset > eligible.len() {
        return Err(cursor_error(
            RecallCursorErrorReason::OffsetOutOfRange,
            cursor,
            "invalid page.cursor: continuation offset is out of range",
        ));
    }
    if boundary != Some(boundary_after(eligible, offset).as_str()) {
        return Err(cursor_error(
            RecallCursorErrorReason::SelectionChanged,
            cursor,
            "invalid page.cursor: the reading no longer resumes after the item its last page \
             ended on; restart the recall without page.cursor",
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

pub(super) fn make_cursor(offset: usize, boundary: &str, selection_hash: &str) -> String {
    format!("{CURSOR_VERSION}:{offset}:{boundary}:{selection_hash}")
}
