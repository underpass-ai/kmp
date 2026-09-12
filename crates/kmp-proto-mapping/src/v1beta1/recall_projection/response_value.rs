//! Rendering a wake or ask response as the JSON the recall projection works
//! on, including the projection and truncation envelopes it already carries.

use kmp_proto::v1beta1::{
    AnswerReason, AskResponse, MemoryEvidence, MemoryLabel, RecallProjection, RecallTruncation,
    WakeClaim, WakeResponse,
};
use serde_json::{Map, Value, json};

use super::actions;
use super::normalization::{normalized_answer_reason, normalized_ask_answer};
use super::proof_value::{empty_proof_value, proof_value, temporal_cursor_value};
use super::request_arguments::dimension_selection_value;
use super::scalars::{detail_label, insert_non_empty, insert_timestamp};

pub fn wake_value(response: &WakeResponse) -> Value {
    let wake = response.wake.as_ref();
    let mut value = json!({
        "scope": {
            "selection": ["proof", "resume_cursor", "wake.causal_spine", "wake.guardrails"],
            "context": ["summary", "wake.current_state", "wake.open_loops", "wake.next_actions", "labels"],
            "context_time": "unbounded",
            "dimensions": response.dimension_selection.as_ref().map(dimension_selection_value),
            "request": ["wake.objective"]
        },
        "summary": response.summary,
        "wake": {
            "objective": wake.map(|wake| wake.objective.as_str()).unwrap_or(""),
            "current_state": wake.map(|wake| wake.current_state.clone()).unwrap_or_default(),
            "causal_spine": wake
                .map(|wake| wake.causal_spine.iter().map(wake_claim_value).collect::<Vec<_>>())
                .unwrap_or_default(),
            "open_loops": wake.map(|wake| wake.open_loops.clone()).unwrap_or_default(),
            "next_actions": wake.map(|wake| wake.next_actions.clone()).unwrap_or_default(),
            "guardrails": wake.map(|wake| wake.guardrails.clone()).unwrap_or_default()
        },
        "proof": response.proof.as_ref().map(|proof| proof_value(proof, response.projection.is_none())).unwrap_or_else(empty_proof_value),
        "labels": response.labels.iter().map(memory_label_value).collect::<Vec<_>>(),
        "resume_cursor": response.resume_cursor.as_ref().map(temporal_cursor_value).unwrap_or(Value::Null),
        "warnings": response.warnings
    });
    attach_typed_projection(
        &mut value,
        response.projection.as_ref(),
        response.truncation.as_ref(),
    );
    value
}

pub fn ask_value(response: &AskResponse) -> Value {
    let evidence = response
        .proof
        .as_ref()
        .map(|proof| proof.evidence.as_slice())
        .unwrap_or_default();
    let answer = if response.projection.is_some() {
        response.answer.clone()
    } else {
        normalized_ask_answer(&response.answer, &response.because, evidence)
    };
    let mut value = json!({
        "summary": response.summary,
        "answer": if answer.trim().is_empty() { Value::Null } else { Value::String(answer) },
        "because": response.because.iter().map(|reason| if response.projection.is_some() { raw_answer_reason_value(reason) } else { answer_reason_value(reason, evidence) }).collect::<Vec<_>>(),
        "proof": response.proof.as_ref().map(|proof| proof_value(proof, response.projection.is_none())).unwrap_or_else(empty_proof_value),
        "warnings": response.warnings
    });
    if !response.asked_as.is_empty() {
        value["asked_as"] = Value::String(response.asked_as.clone());
    }
    attach_typed_projection(
        &mut value,
        response.projection.as_ref(),
        response.truncation.as_ref(),
    );
    value
}

fn attach_typed_projection(
    value: &mut Value,
    projection: Option<&RecallProjection>,
    truncation: Option<&RecallTruncation>,
) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if let Some(projection) = projection {
        object.insert("projection".to_string(), projection_value(projection));
    }
    if let Some(truncation) = truncation {
        object.insert("truncation".to_string(), truncation_value(truncation));
    }
}

fn projection_value(projection: &RecallProjection) -> Value {
    let mut sections = Map::new();
    for section in &projection.sections {
        sections.insert(
            section.name.clone(),
            json!({
                "core": section.core,
                "returned_on_page": section.returned_on_page,
                "remaining": section.remaining,
                "eligible": section.eligible,
                "total": section.total
            }),
        );
    }
    let budget = projection.budget.unwrap_or_default();
    let page = projection.page.clone().unwrap_or_default();
    json!({
        "contract": projection.contract,
        "detail": detail_label(projection.detail),
        "budget": {
            "max_bytes": budget.max_bytes,
            "used_bytes": budget.used_bytes,
            "tokens_advisory": budget.tokens_advisory
        },
        "page": {
            "offset": page.offset,
            "returned": page.returned,
            "total": page.total,
            "has_more": page.has_more,
            "next_cursor": page.next_cursor.clone().map(Value::String).unwrap_or(Value::Null),
            "minimum_progress_bytes": page.minimum_progress_bytes
        },
        "sections": sections,
        "excluded_by_detail": projection.excluded_by_detail,
        "selection_omitted": projection.selection_omitted,
        "core_text_shortened": projection.core_text_shortened,
        "next_action": projection.next_call.as_ref().map(actions::call_value).unwrap_or(Value::Null)
    })
}

fn truncation_value(truncation: &RecallTruncation) -> Value {
    let omitted = truncation.omitted.unwrap_or_default();
    json!({
        "truncated": truncation.truncated,
        "token_limit": truncation.token_limit,
        "byte_limit": truncation.byte_limit,
        "omitted": {
            "page_items": omitted.page_items,
            "prior_page_items": omitted.prior_page_items,
            "remaining_page_items": omitted.remaining_page_items,
            "excluded_by_detail": omitted.excluded_by_detail,
            "selection_items": omitted.selection_items,
            "core_text_shortened": omitted.core_text_shortened
        }
    })
}

fn wake_claim_value(claim: &WakeClaim) -> Value {
    json!({"claim": claim.claim, "because": claim.because, "evidence_ref": claim.evidence_ref})
}

fn answer_reason_value(reason: &AnswerReason, evidence: &[MemoryEvidence]) -> Value {
    raw_answer_reason_value(&normalized_answer_reason(reason, evidence))
}

pub(super) fn raw_answer_reason_value(reason: &AnswerReason) -> Value {
    let mut value = Map::new();
    value.insert("claim".to_string(), json!(reason.claim));
    insert_non_empty(&mut value, "evidence", &reason.evidence);
    value.insert("ref".to_string(), json!(reason.r#ref));
    Value::Object(value)
}

fn memory_label_value(label: &MemoryLabel) -> Value {
    let mut value = Map::new();
    insert_non_empty(&mut value, "about", &label.about);
    insert_non_empty(&mut value, "key", &label.key);
    insert_non_empty(&mut value, "value", &label.value);
    value.insert("entries".to_string(), json!(label.entries));
    insert_timestamp(&mut value, "last_observed_at", label.last_observed_at);
    Value::Object(value)
}
