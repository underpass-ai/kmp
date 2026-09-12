//! Choosing which slice of an inspected object fits the ceiling the caller
//! named, and how to ask for the next one.
//!
//! One concept: inspect pagination. The first page returns the stable object; the
//! expandable sections — evidence, links, raw records — are paged in a fixed
//! order behind a cursor bound to the selection. A caller retaining that object
//! may request only its ref on subsequent pages. No reader state is stored.
//!
//! This runs strictly *after* `inspect_from_response` has rendered its answer,
//! and reaches into that answer by key. `render_inspect_page` also recomputes
//! `quality.truncated` and `quality.relationships`, which the mapper owns. The
//! compiler cannot see that ordering now that the two live apart, so nothing
//! here may be called on a value the mapper did not produce.
//!
//! The guidance string is load-bearing: `inspect_full_required_bytes`
//! iterates to a fixed point because `required_bytes` is interpolated into the
//! very text it measures, and the cursor's selection hash covers the argument
//! set and every item. Changing that wording changes page sizes and
//! invalidates issued cursors.

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use kmp_proto_mapping::v1beta1::recall_projection::{DEFAULT_MAX_BYTES, requested_byte_limit};

use super::serialized_size::serialized_len;
use crate::serving::ToolError;

const INSPECT_CURSOR_VERSION: &str = "kmpi1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InspectSection {
    Evidence,
    Outgoing,
    Incoming,
    Raw,
}

impl InspectSection {
    const ALL: [Self; 4] = [Self::Evidence, Self::Outgoing, Self::Incoming, Self::Raw];

    const fn name(self) -> &'static str {
        match self {
            Self::Evidence => "evidence",
            Self::Outgoing => "outgoing",
            Self::Incoming => "incoming",
            Self::Raw => "raw",
        }
    }
}

#[derive(Debug, Clone)]
struct InspectPageItem {
    section: InspectSection,
    value: Value,
    serialized_bytes: usize,
}

/// Keeps the inspected object as a stable core and pages the sections that can
/// grow around it. The expansion order is evidence, outgoing links, incoming
/// links, then raw audit records. Continuations repeat the object by default;
/// page.repeat_object=false reuses the first page's object after validating the
/// unchanged selection, including the full object, against its cursor.
pub(crate) fn enforce_inspect_output_budget(
    mut value: Value,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let limit = requested_byte_limit(arguments).map_err(ToolError::invalid_argument)?;
    let repeat_object = arguments
        .pointer("/page/repeat_object")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if !repeat_object && arguments.pointer("/page/cursor").is_none() {
        return Err(ToolError::invalid_argument(
            "page.repeat_object=false requires page.cursor and the retained object from its first page",
        ));
    }
    // Move the expansions out of the core. Hash their original encodings once,
    // retaining only each exact length for subsequent page-size decisions.
    let mut items = inspect_page_items(&mut value);
    let (selection_hash, core_bytes) = inspect_selection_hash(&value, &mut items, arguments);
    let offset = inspect_page_offset(arguments, &selection_hash, items.len())?;
    let original_bytes = core_bytes + inspect_expansion_bytes(&items, 0, items.len());
    let mut object = std::mem::replace(&mut value["object"], Value::Null);
    let mut object_bytes = serialized_len(&object);
    let required_bytes = inspect_full_required_bytes(
        &value,
        &items,
        &selection_hash,
        arguments,
        original_bytes,
        object_bytes,
    );
    // Hash and size the complete inspection before reusing any part of it.
    if !repeat_object {
        object = json!({"ref": object["ref"]});
        object_bytes = serialized_len(&object);
        value["object_reused"] = json!(true);
    }
    let page_size = |page: &Value| inspect_page_serialized_len(page, &items, object_bytes);
    let render = |count, args: &Value| {
        render_inspect_page(
            &value,
            &items,
            offset,
            count,
            &selection_hash,
            required_bytes,
            args,
        )
    };
    let remaining = items.len().saturating_sub(offset);
    let complete = render(remaining, arguments);
    let complete_bytes = page_size(&complete);
    if complete_bytes <= limit {
        return Ok(materialize_inspect_page(complete, object, items));
    }

    let mut best = render(0, arguments);
    // A non-final prefix grows by whole items; the complete remainder was
    // checked separately because it no longer carries a continuation action.
    let (mut low, mut high) = (1usize, remaining.saturating_sub(1));
    while low <= high {
        let middle = low + (high - low) / 2;
        let candidate = render(middle, arguments);
        if page_size(&candidate) <= limit {
            best = candidate;
            low = middle + 1;
        } else {
            high = middle - 1;
        }
    }
    if best["page"]["returned"] == 0 && remaining > 0 {
        // Size an actual progressing response, including its continuation.
        // Only the decimal allowance can grow during this fixed point.
        let mut retry = arguments.clone();
        let mut required = page_size(&render(1, &retry)).max(512);
        loop {
            retry["budget"]["max_bytes"] = json!(required);
            let measured = page_size(&render(1, &retry));
            if measured <= required {
                break;
            }
            required = measured;
        }
        // The final remainder can be smaller than a one-item continuation
        // because it carries no next action. Prefer a useful normal-size
        // retry over forcing every caller through one item per response.
        required = required.min(complete_bytes);
        let suggested = required.max(required_bytes.min(DEFAULT_MAX_BYTES));
        retry["budget"]["max_bytes"] = json!(suggested);
        best["page"]["minimum_progress_bytes"] = json!(required);
        best["next_actions"] = json!([inspect_action(
            &retry,
            best["page"]["next_cursor"]
                .as_str()
                .expect("pending cursor")
        )]);
        best["warnings"].as_array_mut().expect("warnings").push(json!(
            "the next whole inspection item does not fit; execute next_actions with its negotiated byte allowance, or retain this result as partial if the budget is fixed"
        ));
    }
    if page_size(&best) > limit {
        let warnings = best["warnings"].as_array_mut().expect("warnings");
        warnings.push(json!(""));
        let index = warnings.len() - 1;
        let mut size = 0;
        loop {
            best["warnings"][index] = json!(format!(
                "budget.max_bytes {limit} is below this response's stable floor; returned the {size}-byte floor instead (the full response requires {required_bytes} bytes)"
            ));
            let measured = page_size(&best);
            if measured == size {
                break;
            }
            size = measured;
        }
    }
    Ok(materialize_inspect_page(best, object, items))
}

fn inspect_page_items(value: &mut Value) -> Vec<InspectPageItem> {
    let mut items = Vec::new();
    for section in InspectSection::ALL {
        let values = std::mem::replace(inspect_section_mut(value, section), json!([]));
        if let Value::Array(values) = values {
            items.extend(values.into_iter().map(|value| InspectPageItem {
                section,
                value,
                // Filled from the same encoding used for the selection hash,
                // before any sizing or pagination is allowed to use this item.
                serialized_bytes: 0,
            }));
        }
    }
    items
}

fn inspect_section_mut(value: &mut Value, section: InspectSection) -> &mut Value {
    match section {
        InspectSection::Evidence => &mut value["evidence"],
        InspectSection::Outgoing => &mut value["links"]["outgoing"],
        InspectSection::Incoming => &mut value["links"]["incoming"],
        InspectSection::Raw => &mut value["raw"],
    }
}

fn inspect_selection_hash(
    core: &Value,
    items: &mut [InspectPageItem],
    arguments: &Value,
) -> (String, usize) {
    let mut bound_arguments = arguments.clone();
    if let Some(arguments) = bound_arguments.as_object_mut() {
        arguments.remove("page");
        let remove_budget = arguments
            .get_mut("budget")
            .and_then(Value::as_object_mut)
            .is_some_and(|budget| {
                budget.remove("max_bytes");
                budget.is_empty()
            });
        if remove_budget {
            arguments.remove("budget");
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(INSPECT_CURSOR_VERSION.as_bytes());
    hasher.update(b"\0");
    hasher.update(serde_json::to_vec(&bound_arguments).expect("inspect arguments serialize"));
    hasher.update(b"\0");
    let core_encoding = serde_json::to_vec(core).expect("inspect core serializes");
    let core_bytes = core_encoding.len();
    hasher.update(core_encoding);
    for item in items {
        hasher.update(b"\0");
        hasher.update(item.section.name().as_bytes());
        hasher.update(b"\0");
        let encoding = serde_json::to_vec(&item.value).expect("inspect item serializes");
        item.serialized_bytes = encoding.len();
        hasher.update(encoding);
    }
    (format!("{:x}", hasher.finalize()), core_bytes)
}

fn inspect_page_offset(
    arguments: &Value,
    selection_hash: &str,
    total: usize,
) -> Result<usize, ToolError> {
    let Some(cursor) = arguments.pointer("/page/cursor") else {
        return Ok(0);
    };
    let Some(cursor) = cursor.as_str().filter(|cursor| !cursor.is_empty()) else {
        return Err(ToolError::invalid_argument(
            "page.cursor must be a non-empty inspect next_cursor",
        ));
    };
    let mut parts = cursor.split(':');
    if parts.next() != Some(INSPECT_CURSOR_VERSION) {
        return Err(ToolError::invalid_argument(
            "invalid page.cursor: malformed inspect continuation",
        ));
    }
    let offset = parts
        .next()
        .and_then(|offset| offset.parse::<usize>().ok())
        .ok_or_else(|| {
            ToolError::invalid_argument(
                "invalid page.cursor: malformed inspect continuation offset",
            )
        })?;
    let hash = parts.next();
    if hash.is_none() || parts.next().is_some() {
        return Err(ToolError::invalid_argument(
            "invalid page.cursor: malformed inspect continuation",
        ));
    }
    if hash != Some(selection_hash) {
        let mut restart = arguments.clone();
        // A restart must return the changed object, even after a reused page.
        restart.as_object_mut().expect("arguments").remove("page");
        return Err(ToolError::conflict(
            "page.cursor does not match this inspect selection; restart the read",
        )
        .with_feedback(
            json!({"code":"READ_SELECTION_CHANGED","field":"page.cursor",
            "action":{"tool":"kmp_inspect","arguments":restart}}),
        ));
    }
    if offset >= total {
        return Err(ToolError::invalid_argument(
            "invalid page.cursor: inspect continuation is exhausted or out of range",
        ));
    }
    Ok(offset)
}

fn inspect_full_required_bytes(
    value: &Value,
    items: &[InspectPageItem],
    selection_hash: &str,
    arguments: &Value,
    original_bytes: usize,
    object_bytes: usize,
) -> usize {
    let mut required = original_bytes;
    for _ in 0..8 {
        let candidate = render_inspect_page(
            value,
            items,
            0,
            items.len(),
            selection_hash,
            required,
            arguments,
        );
        let measured = inspect_page_serialized_len(&candidate, items, object_bytes);
        if measured == required {
            return measured;
        }
        required = measured;
    }
    required
}

fn render_inspect_page(
    value: &Value,
    items: &[InspectPageItem],
    offset: usize,
    keep: usize,
    selection_hash: &str,
    required_bytes: usize,
    arguments: &Value,
) -> Value {
    // This template contains null in place of the object and empty expansion
    // arrays. Large bodies are restored only after the page has been chosen.
    let mut page = value.clone();
    let end = offset.saturating_add(keep).min(items.len());
    let has_more = end < items.len();
    let partial = offset > 0 || has_more;
    let mut omitted = Map::new();
    omitted.insert("details".to_string(), json!(0));
    let mut sections = Map::new();
    let mut relationships = 0;
    for section in InspectSection::ALL {
        let total = items.iter().filter(|item| item.section == section).count();
        let returned = items[offset.min(items.len())..end]
            .iter()
            .filter(|item| item.section == section)
            .count();
        let remaining = items[end..]
            .iter()
            .filter(|item| item.section == section)
            .count();
        if matches!(section, InspectSection::Outgoing | InspectSection::Incoming) {
            relationships += returned;
        }
        omitted.insert(section.name().to_string(), json!(remaining));
        sections.insert(
            section.name().to_string(),
            json!({
                "returned_on_page": returned,
                "remaining": remaining,
                "total": total
            }),
        );
    }
    let next_cursor = has_more.then(|| format!("{INSPECT_CURSOR_VERSION}:{end}:{selection_hash}"));
    let guidance = if has_more {
        Some(format!(
            "Inspect is partial. Execute next_actions and combine the returned expansion items. \
             The full response requires {required_bytes} bytes; minimum_progress_bytes, when \
             present, is the allowance for at least one next item."
        ))
    } else if offset > 0 {
        Some(
            "Final inspect continuation page; combine its expansion items with the stable object \
             and earlier pages."
                .to_string(),
        )
    } else {
        None
    };
    page["next_actions"] = next_cursor
        .as_deref()
        .map(|cursor| json!([inspect_action(arguments, cursor)]))
        .unwrap_or_else(|| json!([]));
    page["page"] = json!({
        "offset": offset,
        "returned": end.saturating_sub(offset),
        "total": items.len(),
        "has_more": has_more,
        "next_cursor": next_cursor,
        "omitted": omitted,
        "sections": sections,
        "required_bytes": required_bytes,
        "guidance": guidance
    });
    if let Some(quality) = page.get_mut("quality").and_then(Value::as_object_mut) {
        quality.insert("truncated".to_string(), json!(partial));
        quality.insert("relationships".to_string(), json!(relationships));
    }
    page
}

// Each array already contributes its brackets in the empty template. Only
// serde's item bytes and the commas between items in that SAME array are added.
fn inspect_expansion_bytes(items: &[InspectPageItem], offset: usize, keep: usize) -> usize {
    let end = offset.saturating_add(keep).min(items.len());
    let mut bytes = 0;
    let mut previous_section = None;
    for item in &items[offset.min(items.len())..end] {
        if previous_section == Some(item.section) {
            bytes += 1;
        }
        bytes += item.serialized_bytes;
        previous_section = Some(item.section);
    }
    bytes
}

fn inspect_page_serialized_len(
    page: &Value,
    items: &[InspectPageItem],
    object_bytes: usize,
) -> usize {
    let offset = page["page"]["offset"].as_u64().expect("inspect offset") as usize;
    let keep = page["page"]["returned"].as_u64().expect("inspect returned") as usize;
    // The object's key and colon are already present. Replace precisely the
    // four bytes of JSON null with the object's canonical encoding length.
    serialized_len(page) - 4 + object_bytes + inspect_expansion_bytes(items, offset, keep)
}

fn materialize_inspect_page(mut page: Value, object: Value, items: Vec<InspectPageItem>) -> Value {
    page["object"] = object;
    let offset = page["page"]["offset"].as_u64().expect("inspect offset") as usize;
    let keep = page["page"]["returned"].as_u64().expect("inspect returned") as usize;
    for item in items.into_iter().skip(offset).take(keep) {
        inspect_section_mut(&mut page, item.section)
            .as_array_mut()
            .expect("inspect expansion section is an array")
            .push(item.value);
    }
    page
}

fn inspect_action(arguments: &Value, cursor: &str) -> Value {
    let mut next = arguments.clone();
    next["page"]["cursor"] = json!(cursor);
    json!({"tool":"kmp_inspect","arguments":next})
}

#[cfg(test)]
#[path = "inspect_budget_tests.rs"]
mod tests;
