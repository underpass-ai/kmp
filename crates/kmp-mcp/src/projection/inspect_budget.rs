//! Choosing which slice of an inspected object fits the ceiling the caller
//! named, and how to ask for the next one.
//!
//! One concept: inspect pagination. The stable object is always returned; the
//! expandable sections — evidence, links, raw records — are paged in a fixed
//! order behind a cursor bound to the selection.
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

use kmp_proto_mapping::v1beta1::recall_projection::requested_byte_limit;

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
}

/// Keeps the inspected object as a stable core and pages the sections that can
/// grow around it. The expansion order is evidence, outgoing links, incoming
/// links, then raw audit records. Every continuation repeats the object and
/// advances through that one ordered selection without repeats or gaps.
pub(crate) fn enforce_inspect_output_budget(
    value: Value,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let limit = requested_byte_limit(arguments).map_err(ToolError::invalid_argument)?;
    let items = inspect_page_items(&value);
    let selection_hash = inspect_selection_hash(&value, &items, arguments);
    let offset = inspect_page_offset(arguments, &selection_hash, items.len())?;
    let required_bytes = inspect_full_required_bytes(&value, &items, &selection_hash);
    let full = render_inspect_page(
        &value,
        &items,
        0,
        items.len(),
        &selection_hash,
        required_bytes,
    );
    if offset == 0 && serialized_len(&full) <= limit {
        return Ok(full);
    }

    let remaining = items.len().saturating_sub(offset);
    let empty = render_inspect_page(&value, &items, offset, 0, &selection_hash, required_bytes);
    let mut best = (serialized_len(&empty) <= limit).then_some(empty);

    // Every non-final candidate carries the same guidance shape and grows as
    // items are appended, so find the largest fitting prefix in logarithmic
    // probes. Test the final page separately because its shorter guidance can
    // make the complete remainder fit when the preceding partial did not.
    let (mut low, mut high) = (1usize, remaining.saturating_sub(1));
    while low <= high {
        let middle = low + (high - low) / 2;
        let candidate = render_inspect_page(
            &value,
            &items,
            offset,
            middle,
            &selection_hash,
            required_bytes,
        );
        if serialized_len(&candidate) <= limit {
            best = Some(candidate);
            low = middle.saturating_add(1);
        } else if middle == 0 {
            break;
        } else {
            high = middle - 1;
        }
    }
    if remaining > 0 {
        let final_page = render_inspect_page(
            &value,
            &items,
            offset,
            remaining,
            &selection_hash,
            required_bytes,
        );
        if serialized_len(&final_page) <= limit {
            best = Some(final_page);
        }
    }

    match best {
        Some(page) => Ok(page),
        None => {
            // The stable object is always returned by design, so when even
            // its floor exceeds the ceiling, return the floor and say so —
            // the contract recall adopted in #439 and temporal in #441.
            let mut floor =
                render_inspect_page(&value, &items, offset, 0, &selection_hash, required_bytes);
            let floor_bytes = serialized_len(&floor);
            if let Some(warnings) = floor["warnings"].as_array_mut() {
                warnings.push(serde_json::json!(format!(
                    "budget.max_bytes {limit} is below this response's stable floor; returned \
                     the {floor_bytes}-byte floor instead — raise max_bytes past it to see \
                     more (the full response requires {required_bytes} bytes)"
                )));
            }
            Ok(floor)
        }
    }
}

fn inspect_page_items(value: &Value) -> Vec<InspectPageItem> {
    let values = |section| match section {
        InspectSection::Evidence => value.get("evidence").and_then(Value::as_array),
        InspectSection::Outgoing => value.pointer("/links/outgoing").and_then(Value::as_array),
        InspectSection::Incoming => value.pointer("/links/incoming").and_then(Value::as_array),
        InspectSection::Raw => value.get("raw").and_then(Value::as_array),
    };
    InspectSection::ALL
        .into_iter()
        .flat_map(|section| {
            values(section)
                .into_iter()
                .flatten()
                .cloned()
                .map(move |value| InspectPageItem { section, value })
        })
        .collect()
}

fn inspect_selection_hash(value: &Value, items: &[InspectPageItem], arguments: &Value) -> String {
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
    let mut core = value.clone();
    core["evidence"] = json!([]);
    core["links"]["outgoing"] = json!([]);
    core["links"]["incoming"] = json!([]);
    core["raw"] = json!([]);
    hasher.update(serde_json::to_vec(&core).expect("inspect core serializes"));
    for item in items {
        hasher.update(b"\0");
        hasher.update(item.section.name().as_bytes());
        hasher.update(b"\0");
        hasher.update(serde_json::to_vec(&item.value).expect("inspect item serializes"));
    }
    format!("{:x}", hasher.finalize())
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
        return Err(ToolError::invalid_argument(
            "invalid page.cursor: it does not match this inspect selection",
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
) -> usize {
    let mut required = serialized_len(value);
    for _ in 0..8 {
        let candidate = render_inspect_page(value, items, 0, items.len(), selection_hash, required);
        let measured = serialized_len(&candidate);
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
) -> Value {
    let mut page = value.clone();
    page["evidence"] = json!([]);
    page["links"]["outgoing"] = json!([]);
    page["links"]["incoming"] = json!([]);
    page["raw"] = json!([]);

    let end = offset.saturating_add(keep).min(items.len());
    for item in &items[offset.min(items.len())..end] {
        let target = match item.section {
            InspectSection::Evidence => &mut page["evidence"],
            InspectSection::Outgoing => &mut page["links"]["outgoing"],
            InspectSection::Incoming => &mut page["links"]["incoming"],
            InspectSection::Raw => &mut page["raw"],
        };
        target
            .as_array_mut()
            .expect("inspect expansion section is an array")
            .push(item.value.clone());
    }

    let has_more = end < items.len();
    let partial = offset > 0 || has_more;
    let mut omitted = Map::new();
    omitted.insert("details".to_string(), json!(0));
    let mut sections = Map::new();
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
            "Inspect is partial. Repeat the same bound arguments with page.cursor set to the \
             returned next_cursor. The full response requires {required_bytes} bytes; narrow \
             include.details/include.outgoing/include.incoming/include.raw or raise \
             budget.max_bytes to restart with more context."
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
    let relationships = page["links"]["incoming"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default()
        + page["links"]["outgoing"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default();
    if let Some(quality) = page.get_mut("quality").and_then(Value::as_object_mut) {
        quality.insert("truncated".to_string(), json!(partial));
        quality.insert("relationships".to_string(), json!(relationships));
    }
    page
}
