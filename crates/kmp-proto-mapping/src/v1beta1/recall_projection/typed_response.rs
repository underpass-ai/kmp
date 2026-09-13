//! Reading projected JSON back onto the typed response, keeping only what
//! the page actually carried and never inventing evidence it dropped.

use std::collections::BTreeSet;

use kmp_proto::v1beta1::{
    AskResponse, MemoryEvidence, MemoryLabel, RecallOmitted, RecallProjection,
    RecallProjectionBudget, RecallProjectionPage, RecallProjectionSection, RecallTruncation,
    SupersededMemory, WakeClaim, WakeResponse,
};
use prost_types::Timestamp;
use serde_json::Value;

use super::actions;
use super::normalization::{normalized_answer_reason, normalized_proof_relation};
use super::proof_value::{expired_value, memory_evidence_value, memory_relation_value};
use super::response_value::raw_answer_reason_value;
use super::scalars::{
    confidence_from_label, detail_from_label, string_at, strings_at, u32_at, u64_at,
};

pub(super) fn apply_wake_value(mut response: WakeResponse, value: &Value) -> WakeResponse {
    response.summary = string_at(value, "/summary");
    if let Some(wake) = response.wake.as_mut() {
        wake.objective = string_at(value, "/wake/objective");
        wake.current_state = strings_at(value, "/wake/current_state");
        wake.open_loops = strings_at(value, "/wake/open_loops");
        wake.next_actions = strings_at(value, "/wake/next_actions");
        wake.guardrails = strings_at(value, "/wake/guardrails");
        wake.causal_spine = value
            .pointer("/wake/causal_spine")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|claim| WakeClaim {
                claim: string_at(claim, "/claim"),
                because: string_at(claim, "/because"),
                evidence_ref: string_at(claim, "/evidence_ref"),
            })
            .collect();
    }
    if let Some(proof) = response.proof.as_mut()
        && let Some(projected) = value.get("proof")
    {
        apply_proof_value(proof, projected);
    }
    response.labels = value
        .get("labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(memory_label_from_value)
        .collect();
    response.warnings = strings_at(value, "/warnings");
    response.projection = value.get("projection").and_then(projection_from_value);
    response.truncation = value.get("truncation").and_then(truncation_from_value);
    response
}

pub(super) fn apply_ask_value(mut response: AskResponse, value: &Value) -> AskResponse {
    response.summary = string_at(value, "/summary");
    response.asked_as = string_at(value, "/asked_as");
    response.answer = value
        .get("answer")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let projected_reasons = value
        .get("because")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let evidence = response
        .proof
        .as_ref()
        .map(|proof| proof.evidence.as_slice())
        .unwrap_or_default();
    let normalized = response
        .because
        .iter()
        .map(|reason| normalized_answer_reason(reason, evidence))
        .collect::<Vec<_>>();
    response.because = projected_reasons
        .iter()
        .filter_map(|wanted| {
            let mut reason = normalized
                .iter()
                .find(|reason| {
                    wanted["ref"].as_str() == Some(reason.r#ref.as_str())
                        && wanted["claim"].as_str() == Some(reason.claim.as_str())
                })?
                .clone();
            reason.evidence = wanted
                .get("evidence")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            (raw_answer_reason_value(&reason) == *wanted).then_some(reason)
        })
        .collect();
    if let Some(proof) = response.proof.as_mut()
        && let Some(projected) = value.get("proof")
    {
        apply_proof_value(proof, projected);
    }
    response.warnings = strings_at(value, "/warnings");
    response.projection = value.get("projection").and_then(projection_from_value);
    response.truncation = value.get("truncation").and_then(truncation_from_value);
    response
}

fn apply_proof_value(proof: &mut kmp_proto::v1beta1::Proof, value: &Value) {
    let normalized_relations = proof
        .path
        .iter()
        .map(|relation| normalized_proof_relation(relation, &proof.evidence))
        .collect::<Vec<_>>();
    let path = value
        .get("path")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    proof.path = select_projected(&normalized_relations, &path, memory_relation_value);
    let evidence = value
        .get("evidence")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    proof.evidence = select_projected_evidence(&proof.evidence, &evidence);
    let superseded = value
        .get("superseded")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    proof.superseded = select_projected_superseded(&proof.superseded, &superseded);
    let expired = value
        .get("expired")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    proof.expired = select_projected(&proof.expired, &expired, expired_value);
    proof.conflicts = strings_at(value, "/conflicts");
    proof.missing = strings_at(value, "/missing");
    proof.matched_terms = strings_at(value, "/matched_terms");
    proof.matched_relations = strings_at(value, "/matched_relations");
    proof.frontier_size = value
        .get("frontier_size")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_default();
    proof.confidence = confidence_from_label(
        value
            .get("confidence")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
    );
}

// Evidence bodies may be shortened in the stable core. Reconstruct the
// planned body while requiring every identity/provenance field to still match;
// equality against the unshortened body would silently drop a cited object.
fn select_projected_evidence(
    originals: &[MemoryEvidence],
    projected: &[Value],
) -> Vec<MemoryEvidence> {
    let mut used = BTreeSet::new();
    projected
        .iter()
        .filter_map(|wanted| {
            let id = wanted.get("id")?.as_str()?;
            let text = wanted.get("text")?.as_str()?;
            let original = originals.iter().find(|item| item.id == id)?;
            let mut selected = original.clone();
            selected.text = text.to_string();
            if memory_evidence_value(&selected) != *wanted || !used.insert(id.to_string()) {
                return None;
            }
            Some(selected)
        })
        .collect()
}

fn select_projected<T: Clone>(
    originals: &[T],
    projected: &[Value],
    render: impl Fn(&T) -> Value,
) -> Vec<T> {
    let mut used = vec![false; originals.len()];
    projected
        .iter()
        .filter_map(|wanted| {
            let index = originals
                .iter()
                .enumerate()
                .find(|(index, item)| !used[*index] && render(item) == *wanted)
                .map(|(index, _)| index)?;
            used[index] = true;
            Some(originals[index].clone())
        })
        .collect()
}

fn select_projected_superseded(
    originals: &[SupersededMemory],
    projected: &[Value],
) -> Vec<SupersededMemory> {
    let mut used = vec![false; originals.len()];
    projected
        .iter()
        .filter_map(|wanted| {
            let r#ref = wanted.get("ref").and_then(Value::as_str)?;
            let superseded_by = wanted.get("superseded_by").and_then(Value::as_str)?;
            let index = originals
                .iter()
                .enumerate()
                .find(|(index, item)| {
                    !used[*index] && item.r#ref == r#ref && item.superseded_by == superseded_by
                })
                .map(|(index, _)| index)?;
            used[index] = true;
            let mut selected = originals[index].clone();
            selected.why = wanted
                .get("why")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Some(selected)
        })
        .collect()
}

fn projection_from_value(value: &Value) -> Option<RecallProjection> {
    let sections = value
        .get("sections")?
        .as_object()?
        .iter()
        .map(|(name, section)| RecallProjectionSection {
            name: name.clone(),
            core: u64_at(section, "/core"),
            returned_on_page: u64_at(section, "/returned_on_page"),
            remaining: u64_at(section, "/remaining"),
            eligible: u64_at(section, "/eligible"),
            total: u64_at(section, "/total"),
        })
        .collect();
    Some(RecallProjection {
        contract: string_at(value, "/contract"),
        detail: detail_from_label(value.get("detail")?.as_str()?),
        budget: Some(RecallProjectionBudget {
            max_bytes: u64_at(value, "/budget/max_bytes"),
            used_bytes: u64_at(value, "/budget/used_bytes"),
            tokens_advisory: u32_at(value, "/budget/tokens_advisory"),
        }),
        page: Some(RecallProjectionPage {
            minimum_progress_bytes: value
                .pointer("/page/minimum_progress_bytes")
                .and_then(Value::as_u64),
            offset: u64_at(value, "/page/offset"),
            returned: u64_at(value, "/page/returned"),
            total: u64_at(value, "/page/total"),
            has_more: value
                .pointer("/page/has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            next_cursor: value
                .pointer("/page/next_cursor")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        }),
        sections,
        excluded_by_detail: u64_at(value, "/excluded_by_detail"),
        selection_omitted: u64_at(value, "/selection_omitted"),
        core_text_shortened: value
            .get("core_text_shortened")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        next_action: None,
        next_call: value
            .get("next_action")
            .filter(|value| value.is_object())
            .map(actions::call_from_value),
    })
}

fn truncation_from_value(value: &Value) -> Option<RecallTruncation> {
    Some(RecallTruncation {
        truncated: value.get("truncated")?.as_bool()?,
        token_limit: u32_at(value, "/token_limit"),
        byte_limit: u64_at(value, "/byte_limit"),
        omitted: Some(RecallOmitted {
            page_items: u64_at(value, "/omitted/page_items"),
            prior_page_items: u64_at(value, "/omitted/prior_page_items"),
            remaining_page_items: u64_at(value, "/omitted/remaining_page_items"),
            excluded_by_detail: u64_at(value, "/omitted/excluded_by_detail"),
            selection_items: u64_at(value, "/omitted/selection_items"),
            core_text_shortened: value
                .pointer("/omitted/core_text_shortened")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
    })
}

fn memory_label_from_value(value: &Value) -> MemoryLabel {
    MemoryLabel {
        about: string_at(value, "/about"),
        key: string_at(value, "/key"),
        value: string_at(value, "/value"),
        entries: u32_at(value, "/entries"),
        last_observed_at: value
            .pointer("/last_observed_at")
            .and_then(Value::as_str)
            .and_then(|instant| instant.parse::<Timestamp>().ok()),
    }
}
