//! Reading a recall request back as lossless JSON arguments, so a cursor
//! and a proposed continuation mean exactly what the caller asked for.

use kmp_proto::v1beta1::{
    AskRequest, DimensionScopeMode, DimensionSelection, DimensionSelectionMode, WakeRequest,
};
use serde_json::{Map, Value, json};

use super::actions;
use super::budget::DEFAULT_MAX_BYTES;
use super::scalars::{detail_label, insert_non_empty};

/// Reads `budget.max_bytes` the way the recall projection does, so the
/// temporal verbs cannot mean something different by the same argument.
pub fn requested_byte_limit(arguments: &Value) -> Result<usize, String> {
    match arguments.pointer("/budget/max_bytes") {
        None | Some(Value::Null) => Ok(DEFAULT_MAX_BYTES),
        Some(value) => value
            .as_u64()
            .and_then(|bytes| usize::try_from(bytes).ok())
            .filter(|bytes| *bytes >= 512)
            .ok_or_else(|| "budget.max_bytes must be an integer of at least 512".to_string()),
    }
}

pub(super) fn wake_arguments(request: &WakeRequest) -> Value {
    let mut arguments = Map::new();
    arguments.insert("about".to_string(), json!(request.about));
    insert_non_empty(&mut arguments, "role", &request.role);
    insert_non_empty(&mut arguments, "intent", &request.intent);
    arguments.insert(
        "budget".to_string(),
        budget_value(request.budget.as_ref(), 1_600, 2),
    );
    if let Some(dimensions) = request.dimensions.as_ref() {
        arguments.insert(
            "dimensions".to_string(),
            dimension_selection_value(dimensions),
        );
    }
    if let Some(page) = request.page.as_ref() {
        arguments.insert("page".to_string(), page_request_value(page));
    }
    actions::insert_time(
        &mut arguments,
        request.axis,
        request.as_of.as_ref(),
        request.interval.as_ref(),
    );
    Value::Object(arguments)
}

pub(super) fn ask_arguments(request: &AskRequest) -> Value {
    let mut arguments = Map::new();
    arguments.insert("about".to_string(), json!(request.about));
    arguments.insert("question".to_string(), json!(request.question));
    insert_non_empty(&mut arguments, "asked_as", &request.asked_as);
    arguments.insert(
        "answer_policy".to_string(),
        json!(
            match kmp_proto::v1beta1::AnswerPolicy::try_from(request.answer_policy) {
                Ok(kmp_proto::v1beta1::AnswerPolicy::ShowConflicts) => "show_conflicts",
                Ok(kmp_proto::v1beta1::AnswerPolicy::BestEffort) => "best_effort",
                _ => "evidence_or_unknown",
            }
        ),
    );
    arguments.insert(
        "budget".to_string(),
        budget_value(request.budget.as_ref(), 2_400, 2),
    );
    if let Some(dimensions) = request.dimensions.as_ref() {
        arguments.insert(
            "dimensions".to_string(),
            dimension_selection_value(dimensions),
        );
    }
    if let Some(page) = request.page.as_ref() {
        arguments.insert("page".to_string(), page_request_value(page));
    }
    actions::insert_time(
        &mut arguments,
        request.axis,
        request.as_of.as_ref(),
        request.interval.as_ref(),
    );
    Value::Object(arguments)
}

fn budget_value(
    budget: Option<&kmp_proto::v1beta1::MemoryBudget>,
    default_tokens: u32,
    default_depth: u32,
) -> Value {
    let budget = budget.cloned().unwrap_or_default();
    let mut value = json!({
        "tokens": if budget.tokens == 0 { default_tokens } else { budget.tokens },
        "detail": detail_label(budget.detail),
        "depth": if budget.depth == 0 { default_depth } else { budget.depth },
        "max_entries": budget.max_entries,
        "max_bytes": if budget.max_bytes == 0 {
            DEFAULT_MAX_BYTES as u64
        } else {
            budget.max_bytes
        }
    });
    if budget.max_entries == 0 {
        value
            .as_object_mut()
            .expect("budget object")
            .remove("max_entries");
    }
    value
}

pub(super) fn dimension_selection_value(selection: &DimensionSelection) -> Value {
    let mut value = json!({
        "mode": match DimensionSelectionMode::try_from(selection.mode) {
            Ok(DimensionSelectionMode::Only) => "only",
            Ok(DimensionSelectionMode::Except) => "except",
            _ => "all",
        },
        "include": selection.include,
        "exclude": selection.exclude,
        "scope": match DimensionScopeMode::try_from(selection.scope) {
            Ok(DimensionScopeMode::Abouts) => "abouts",
            Ok(DimensionScopeMode::AllAbouts) => "all_abouts",
            _ => "current_about",
        },
        "abouts": selection.abouts,
        "scope_ids": selection.scope_ids,
        "selectors": selection.selectors.iter().map(|selector| {
            let mut value = json!({"key":selector.key,"op":match kmp_proto::v1beta1::LabelSelectorOperator::try_from(selector.op) {
                Ok(kmp_proto::v1beta1::LabelSelectorOperator::In) => "in",
                Ok(kmp_proto::v1beta1::LabelSelectorOperator::NotIn) => "notin",
                Ok(kmp_proto::v1beta1::LabelSelectorOperator::Exists) => "exists",
                Ok(kmp_proto::v1beta1::LabelSelectorOperator::NotExists) => "notexists",
                _ => "unspecified",
            }});
            if !selector.values.is_empty() { value["values"] = json!(selector.values); }
            value
        }).collect::<Vec<_>>()
    });
    value
        .as_object_mut()
        .expect("dimensions")
        .retain(|_, value| !value.as_array().is_some_and(Vec::is_empty));
    value
}

fn page_request_value(page: &kmp_proto::v1beta1::PageRequest) -> Value {
    let mut value = Map::new();
    if page.entries != 0 {
        value.insert("entries".to_string(), json!(page.entries));
    }
    insert_non_empty(&mut value, "cursor", &page.cursor);
    Value::Object(value)
}
