//! The projection envelope a recall page carries: what was returned, what
//! remains, why anything else is absent, and the call that reads the rest.
//! It is the page's one progress block; every omission cause has its own
//! counter here (prior pages in `page.offset`, pending delivery in
//! `sections.*.remaining`, per-section detail exclusions in
//! `sections.*.excluded_by_detail`, detail in `excluded_by_detail`, the entries cap in
//! `selection_omitted`, shortened prose in `core_text_shortened`).

use std::borrow::Borrow;

use serde_json::{Map, Value, json};

use super::actions;
use super::budget::{DEFAULT_MAX_BYTES, ProjectionBudget};
use super::cursor::make_cursor;
use super::plan::{ProjectionItem, ProjectionPlan, Section};
use super::reused_core::reuses_core;

pub const PROJECTION_CONTRACT: &str = "kmp.recall.projection.v3";

#[allow(clippy::too_many_arguments)]
pub(super) fn attach_metadata<E, S>(
    value: &mut Value,
    plan: &ProjectionPlan,
    eligible: &[E],
    selected: &[S],
    offset: usize,
    excluded_by_detail: usize,
    selection_hash: &str,
    budget: &ProjectionBudget,
    core_text_shortened: bool,
    planning: bool,
) where
    E: Borrow<ProjectionItem>,
    S: Borrow<ProjectionItem>,
{
    const RESTART: &str = "recall core prose was shortened; execute projection.next_action to restart and discard the partial reconstruction before reading more expansion";
    const STALLED: &str = "recall expansion cannot advance at this byte budget; repeating the same cursor with unchanged budget.max_bytes returns no additional evidence";
    const PARTIAL: &str = "recall expansion pending; see projection.sections.*.remaining and execute projection.next_action";
    const DETAIL: &str = "recall detail excludes expansion items; start a fresh recall with a richer budget.detail to include them";
    const CAPPED: &str = "recall selection was capped by budget.max_entries; start a fresh recall with a larger cap to include those items";
    const FINAL: &str = "final continuation page; combine its expansion items with the stable core and earlier pages";
    let next_offset = offset.saturating_add(selected.len());
    let has_more = next_offset < eligible.len();
    let reported_offset = if planning { usize::MAX } else { offset };
    let cursor = if planning {
        make_cursor(usize::MAX, &"f".repeat(64))
    } else if has_more {
        make_cursor(next_offset, selection_hash)
    } else {
        String::new()
    };
    // Lean progress (#544 C3): a section reports only counts a host cannot
    // derive, and never a zero. `core` and `excluded_by_detail` are static:
    // the page that carries the core states them, a continuation that reuses
    // it does not. `eligible` is `core` plus every page's `returned_on_page`
    // plus the last `remaining`; `total` is `eligible + excluded_by_detail`.
    // Planning sizes every counter at its largest, so it bounds all pages.
    let static_counts = planning || !reuses_core(offset, budget.repeat_core);
    let mut sections = Map::new();
    for section in Section::ALL {
        let core = plan.core_lengths.get(&section).copied().unwrap_or(0);
        let total = core
            + plan
                .items
                .iter()
                .filter(|item| item.section == section)
                .count();
        if total == 0 {
            continue;
        }
        let eligible_total = core
            + eligible
                .iter()
                .filter(|item| <E as Borrow<ProjectionItem>>::borrow(*item).section == section)
                .count();
        let returned = selected
            .iter()
            .filter(|item| <S as Borrow<ProjectionItem>>::borrow(*item).section == section)
            .count();
        let remaining = eligible[next_offset.min(eligible.len())..]
            .iter()
            .filter(|item| <E as Borrow<ProjectionItem>>::borrow(*item).section == section)
            .count();
        let mut counts = Map::new();
        let mut insert = |key: &str, count: usize| {
            if count > 0 {
                counts.insert(key.to_string(), json!(count));
            }
        };
        if static_counts {
            insert("core", if planning { total } else { core });
        }
        insert("returned_on_page", if planning { total } else { returned });
        insert("remaining", if planning { total } else { remaining });
        if static_counts {
            insert(
                "excluded_by_detail",
                if planning {
                    total
                } else {
                    total - eligible_total
                },
            );
        }
        if !counts.is_empty() {
            sections.insert(section.name().to_string(), Value::Object(counts));
        }
    }
    let truncated = planning
        || has_more
        || offset > 0
        || excluded_by_detail > 0
        || plan.selection_omitted > 0
        || core_text_shortened;
    let stalled = !planning && has_more && selected.is_empty();
    let next_action = (planning || has_more || core_text_shortened).then(|| {
        let max_bytes = if planning {
            usize::MAX
        } else if stalled || core_text_shortened {
            plan.progress_bytes
                .saturating_add(DEFAULT_MAX_BYTES)
                .max(budget.byte_limit)
        } else {
            budget.byte_limit
        };
        // A shortened core cannot be combined as if it were the full first
        // page. Restart the same selection at a sufficient allowance.
        let continuation = if core_text_shortened && !planning {
            None
        } else {
            Some(cursor.as_str())
        };
        if planning {
            // Size against one envelope whatever tier was asked for: the
            // longest detail word, so the fitted core never depends on it.
            let mut arguments = plan.arguments.clone();
            if arguments.is_object() {
                arguments["budget"]["detail"] = json!(super::budget::LONGEST_DETAIL);
            }
            actions::call(&arguments, continuation, Some(max_bytes))
        } else {
            actions::call(&plan.arguments, continuation, Some(max_bytes))
        }
    });
    value["projection"] = json!({
        "contract": PROJECTION_CONTRACT,
        "detail": budget.detail.as_str(),
        "budget": {
            "max_bytes": budget.byte_limit,
            "used_bytes": budget.byte_limit,
            "tokens_advisory": budget.token_limit
        },
        "page": {
            "offset": reported_offset,
            "returned": if planning { eligible.len() } else { selected.len() },
            "total": eligible.len(),
            "has_more": planning || has_more,
            "next_cursor": if cursor.is_empty() { Value::Null } else { json!(cursor) },
            "minimum_progress_bytes": if planning { Some(usize::MAX) } else if stalled || core_text_shortened { Some(plan.progress_bytes) } else { None }
        },
        "sections": sections,
        "excluded_by_detail": excluded_by_detail,
        "selection_omitted": plan.selection_omitted,
        "core_text_shortened": core_text_shortened,
        "next_action": next_action
    });
    if truncated {
        let warning = if planning {
            // Reserve the longest warning we actually emit, rather than a
            // separate planning paragraph that displaces usable evidence.
            [RESTART, STALLED, PARTIAL, DETAIL, CAPPED, FINAL]
                .into_iter()
                .max_by_key(|warning| warning.len())
                .expect("recall warnings")
        } else if core_text_shortened {
            RESTART
        } else if stalled {
            STALLED
        } else if has_more {
            PARTIAL
        } else if excluded_by_detail > 0 {
            DETAIL
        } else if plan.selection_omitted > 0 {
            CAPPED
        } else {
            FINAL
        };
        append_warning(value, warning);
    }
}

pub(super) fn append_warning(value: &mut Value, warning: &str) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let warnings = object
        .entry("warnings")
        .or_insert_with(|| json!([]))
        .as_array_mut();
    if let Some(warnings) = warnings
        && !warnings.iter().any(|item| item.as_str() == Some(warning))
    {
        warnings.push(json!(warning));
    }
}
