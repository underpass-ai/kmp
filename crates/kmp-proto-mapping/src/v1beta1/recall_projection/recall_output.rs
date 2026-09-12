//! The projection itself: plan a recall packet, fit its core, then fill the
//! requested page with as much expansion as the byte ceiling allows.

use kmp_domain::TokenEstimator;
use serde_json::Value;

use super::actions;
use super::budget::{Detail, ProjectionBudget};
use super::core_fit::{fit_core, fits, serialized_bytes, stabilize_used_bytes};
use super::cursor::{parse_cursor, selection_hash};
use super::json_paths::push_array;
use super::metadata::{append_warning, attach_metadata};
use super::plan::{ProjectionPlan, section_lengths};
use super::projection_error::RecallProjectionError;
use super::projection_outcome::ProjectionOutcome;
use super::text_shortening::truncate_json_text;

pub fn project_recall_output(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
    estimator: &dyn TokenEstimator,
) -> Result<ProjectionOutcome, String> {
    project_recall_output_typed(value, arguments, default_tokens, estimator)
        .map_err(|error| error.to_string())
}

pub fn project_recall_output_typed(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
    _estimator: &dyn TokenEstimator,
) -> Result<ProjectionOutcome, RecallProjectionError> {
    let budget = ProjectionBudget::from_arguments(arguments, default_tokens)
        .map_err(RecallProjectionError::InvalidRequest)?;
    let mut plan = ProjectionPlan::build(value, &budget);
    plan.arguments = arguments.clone();
    let eligible = plan
        .items
        .iter()
        .filter(|item| item.min_detail <= budget.detail)
        .cloned()
        .collect::<Vec<_>>();
    let selection_hash = selection_hash(arguments, &plan, &eligible);
    let excluded_by_detail = plan.items.len() - eligible.len();
    let offset = parse_cursor(
        arguments.pointer("/page/cursor").and_then(Value::as_str),
        &selection_hash,
        eligible.len(),
    )
    .map_err(|mut error| {
        if let RecallProjectionError::Cursor { restart, .. } = &mut error {
            *restart = Some(actions::call(arguments, None, None));
        }
        error
    })?;
    plan.progress_bytes = actions::progress_bytes(&plan, &selection_hash, &budget);

    // Size the core against one worst-case metadata envelope so its bytes do
    // not change with detail, cursor offset, or an advisory-token override.
    let core_budget = ProjectionBudget {
        token_limit: u32::MAX,
        detail: Detail::Balanced,
        page_entries: usize::MAX,
        ..budget
    };
    // A ceiling below the stable floor is not an error any more (#439): the
    // caller gets the floor — the zero-text core — with a warning naming it,
    // which costs one call instead of training every caller to over-budget.
    let (core, core_text_shortened, floor_mode) = match fit_core(
        &plan,
        &plan.items,
        0,
        plan.items.len(),
        &selection_hash,
        &core_budget,
    ) {
        Some((core, shortened)) => (core, shortened, false),
        None => {
            let mut zero = plan.core.clone();
            truncate_json_text(&mut zero, 0);
            (zero, true, true)
        }
    };
    plan.core = core;
    plan.core_lengths = section_lengths(&plan.core);

    let mut selected = Vec::new();
    let mut planning = plan.core.clone();
    attach_metadata(
        &mut planning,
        &plan,
        &eligible,
        &[],
        offset,
        excluded_by_detail,
        &selection_hash,
        &budget,
        core_text_shortened,
        true,
    );
    let mut planned_bytes = serialized_bytes(&planning);
    let mut lengths = plan.core_lengths.clone();

    for item in eligible.iter().skip(offset).take(budget.page_entries) {
        let item_json =
            serde_json::to_string(&item.value).expect("projection item should serialize");
        let comma_bytes = usize::from(lengths.get(&item.section).copied().unwrap_or(0) > 0);
        let item_bytes = item_json.len() + comma_bytes;
        // `tokens` predates the transport-neutral byte contract and remains in
        // the API as a planning hint. Treating it as a second hard cap made a
        // typical stable core consume the entire default before any expansion
        // item was considered: compact, balanced and full then returned the
        // same payload while claiming different detail exclusions. The byte
        // ceiling is the normative host-safety boundary; detail and paging
        // choose the expansion inside it.
        if planned_bytes.saturating_add(item_bytes) > budget.byte_limit {
            break;
        }
        planned_bytes += item_bytes;
        *lengths.entry(item.section).or_default() += 1;
        selected.push(item.clone());
    }

    if selected.is_empty() && offset < eligible.len() && !floor_mode {
        // Never manufacture a continuation that cannot advance. `fit_core`
        // reserves one item, so reaching this branch means the hard byte
        // ceiling cannot carry both the stable core and any expansion. In
        // floor mode the response says exactly why it cannot advance, so the
        // caller is never left retrying the same cursor blind.
        return Ok(ProjectionOutcome::CoreTooLarge);
    }

    // Item costs are deliberately conservative, so this normally runs once.
    // The exact final assertion protects the hard byte ceiling and estimator
    // compatibility without the old serialize-and-drop O(n²) loop.
    loop {
        let mut projected = plan.core.clone();
        for item in &selected {
            push_array(&mut projected, item.section.path(), item.value.clone());
        }
        attach_metadata(
            &mut projected,
            &plan,
            &eligible,
            &selected,
            offset,
            excluded_by_detail,
            &selection_hash,
            &budget,
            core_text_shortened,
            false,
        );
        stabilize_used_bytes(&mut projected);
        if fits(&projected, &budget) {
            return Ok(ProjectionOutcome::Projected(projected));
        }
        if selected.pop().is_none() {
            if floor_mode {
                // The floor exceeds the requested ceiling by definition.
                // Return it anyway, saying so: min(content, floor) beats an
                // error the caller can only answer by over-budgeting.
                let floor_bytes = serialized_bytes(&projected);
                append_warning(
                    &mut projected,
                    &format!(
                        "budget.max_bytes {} is below this response's stable floor; returned \
                         the {floor_bytes}-byte floor instead — raise max_bytes past it to \
                         see more",
                        budget.byte_limit
                    ),
                );
                stabilize_used_bytes(&mut projected);
                return Ok(ProjectionOutcome::Projected(projected));
            }
            return Ok(ProjectionOutcome::CoreTooLarge);
        }
    }
}
