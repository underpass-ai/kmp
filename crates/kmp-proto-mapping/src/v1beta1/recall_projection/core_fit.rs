//! Fitting the stable core inside the byte ceiling, and measuring what a
//! candidate projection actually serializes to.

use serde_json::{Value, json};

use super::budget::ProjectionBudget;
use super::json_paths::push_array;
use super::metadata::attach_metadata;
use super::plan::{ProjectionItem, ProjectionPlan};
use super::text_shortening::{max_text_chars, truncate_json_text};

pub(super) fn fit_core(
    plan: &ProjectionPlan,
    eligible: &[ProjectionItem],
    offset: usize,
    excluded_by_detail: usize,
    selection_hash: &str,
    budget: &ProjectionBudget,
) -> Option<(Value, bool)> {
    // A continuation is only useful if it can advance. Reserve enough room
    // for the largest expansion item in the selection while fitting the
    // stable core; otherwise a shortened core can consume the whole budget
    // and produce returned=0, has_more=true, and the same cursor forever.
    // Choosing from the complete canonical plan keeps this core identical
    // across detail levels and page offsets.
    let reserved_item = eligible.iter().max_by_key(|item| {
        item.stable_key.len()
            + usize::from(plan.core_lengths.get(&item.section).copied().unwrap_or(0) > 0)
    });
    let build = |max_chars: Option<usize>| {
        let mut candidate = plan.core.clone();
        if let Some(max_chars) = max_chars {
            truncate_json_text(&mut candidate, max_chars);
        }
        if let Some(item) = reserved_item {
            push_array(&mut candidate, item.section.path(), item.value.clone());
        }
        attach_metadata(
            &mut candidate,
            plan,
            eligible,
            &[],
            offset,
            excluded_by_detail,
            selection_hash,
            budget,
            max_chars.is_some(),
            true,
        );
        candidate
    };
    if fits(&build(None), budget) {
        return Some((plan.core.clone(), false));
    }

    let mut low = 0usize;
    let mut high = max_text_chars(&plan.core);
    let mut best = None;
    while low <= high {
        let midpoint = low + (high - low) / 2;
        if fits(&build(Some(midpoint)), budget) {
            let mut bounded = plan.core.clone();
            truncate_json_text(&mut bounded, midpoint);
            best = Some((bounded, true));
            low = midpoint.saturating_add(1);
        } else if midpoint == 0 {
            break;
        } else {
            high = midpoint - 1;
        }
    }
    best
}

pub(super) fn stabilize_used_bytes(value: &mut Value) {
    for _ in 0..3 {
        let used = serialized_bytes(value);
        if value
            .pointer("/projection/budget/used_bytes")
            .and_then(Value::as_u64)
            == u64::try_from(used).ok()
        {
            break;
        }
        value["projection"]["budget"]["used_bytes"] = json!(used);
    }
}

pub(super) fn fits(value: &Value, budget: &ProjectionBudget) -> bool {
    serialized_bytes(value) <= budget.byte_limit
}

pub(super) fn serialized_bytes(value: &Value) -> usize {
    serde_json::to_vec(value)
        .expect("recall projection should serialize")
        .len()
}
