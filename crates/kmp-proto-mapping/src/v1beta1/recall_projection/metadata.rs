//! The projection and truncation envelope a recall page carries: what was
//! returned, what remains, and the call that reads the rest.

use serde_json::{Map, Value, json};

use super::actions;
use super::budget::{DEFAULT_MAX_BYTES, ProjectionBudget};
use super::cursor::make_cursor;
use super::plan::{ProjectionItem, ProjectionPlan, Section};

pub const PROJECTION_CONTRACT: &str = "kmp.recall.projection.v1";

#[allow(clippy::too_many_arguments)]
pub(super) fn attach_metadata(
    value: &mut Value,
    plan: &ProjectionPlan,
    eligible: &[ProjectionItem],
    selected: &[ProjectionItem],
    offset: usize,
    excluded_by_detail: usize,
    selection_hash: &str,
    budget: &ProjectionBudget,
    core_text_shortened: bool,
    planning: bool,
) {
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
                .filter(|item| item.section == section)
                .count();
        let returned = selected
            .iter()
            .filter(|item| item.section == section)
            .count();
        let remaining = eligible[next_offset.min(eligible.len())..]
            .iter()
            .filter(|item| item.section == section)
            .count();
        sections.insert(
            section.name().to_string(),
            json!({
                "core": core,
                "returned_on_page": if planning { total } else { returned },
                "remaining": if planning { total } else { remaining },
                "eligible": eligible_total,
                "total": total
            }),
        );
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
        actions::call(&plan.arguments, continuation, Some(max_bytes))
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
        let remaining_page_items = eligible.len().saturating_sub(next_offset);
        value["truncation"] = json!({
            "truncated": true,
            "token_limit": budget.token_limit,
            "byte_limit": budget.byte_limit,
            "omitted": {
                "page_items": if planning { eligible.len() } else { eligible.len().saturating_sub(selected.len()) },
                "prior_page_items": if planning { eligible.len() } else { offset },
                "remaining_page_items": if planning { eligible.len() } else { remaining_page_items },
                "excluded_by_detail": excluded_by_detail,
                "selection_items": plan.selection_omitted,
                "core_text_shortened": core_text_shortened
            }
        });
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
    } else if let Some(object) = value.as_object_mut() {
        object.remove("truncation");
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
