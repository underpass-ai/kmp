//! Fitting the stable core inside the byte ceiling, and measuring what a
//! candidate projection actually serializes to.

use serde_json::{Value, json};

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use super::budget::ProjectionBudget;
use super::json_paths::push_array;
use super::metadata::attach_metadata;
use super::plan::{ProjectionItem, ProjectionPlan};
use super::serialized_size::serialized_size;
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
            &[] as &[ProjectionItem],
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
    let mut best_chars = None;
    while low <= high {
        let midpoint = low + (high - low) / 2;
        if fits(&build(Some(midpoint)), budget) {
            best_chars = Some(midpoint);
            low = midpoint.saturating_add(1);
        } else if midpoint == 0 {
            break;
        } else {
            high = midpoint - 1;
        }
    }
    best_chars.map(|max_chars| {
        let mut bounded = plan.core.clone();
        truncate_json_text(&mut bounded, max_chars);
        (bounded, true)
    })
}

pub(super) fn stabilize_used_bytes(value: &mut Value) -> usize {
    let current = value
        .pointer("/projection/budget/used_bytes")
        .and_then(Value::as_u64)
        .expect("projection metadata should contain integer used_bytes");
    let measured = serialized_bytes(value);
    let fixed_without_value = measured
        .checked_sub(decimal_digits_u64(current))
        .expect("serialized projection includes used_bytes");
    let mut used = measured;
    loop {
        let next = fixed_without_value
            .checked_add(decimal_digits_usize(used))
            .expect("serialized recall projection size should fit usize");
        if next == used {
            break;
        }
        used = next;
    }
    value["projection"]["budget"]["used_bytes"] = json!(used);
    used
}

pub(super) fn fits(value: &Value, budget: &ProjectionBudget) -> bool {
    serialized_bytes(value) <= budget.byte_limit
}

pub(super) fn serialized_bytes(value: &Value) -> usize {
    #[cfg(test)]
    FULL_SERIALIZATION_PASSES.fetch_add(1, Ordering::Relaxed);
    serialized_size(value)
}

fn decimal_digits_u64(value: u64) -> usize {
    if value == 0 {
        1
    } else {
        usize::try_from(value.ilog10()).expect("digit count fits usize") + 1
    }
}

fn decimal_digits_usize(value: usize) -> usize {
    if value == 0 {
        1
    } else {
        usize::try_from(value.ilog10()).expect("digit count fits usize") + 1
    }
}

#[cfg(test)]
static FULL_SERIALIZATION_PASSES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static ITEM_SERIALIZATION_PASSES: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
#[allow(dead_code)] // Baseline harness hook: zero calls is the candidate result.
pub(super) fn note_item_serialization() {
    ITEM_SERIALIZATION_PASSES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
pub(super) fn reset_serialization_passes() {
    FULL_SERIALIZATION_PASSES.store(0, Ordering::Relaxed);
    ITEM_SERIALIZATION_PASSES.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(super) fn serialization_passes() -> (usize, usize) {
    (
        FULL_SERIALIZATION_PASSES.load(Ordering::Relaxed),
        ITEM_SERIALIZATION_PASSES.load(Ordering::Relaxed),
    )
}
