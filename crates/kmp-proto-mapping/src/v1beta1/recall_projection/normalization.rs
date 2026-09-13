//! Normalising an answer and its evidence so the same fact is never stored
//! twice in one packet: citations, relation rationales and rebuilt answers.

use std::collections::BTreeSet;

use kmp_proto::v1beta1::{AnswerReason, MemoryEvidence, MemoryRelation, MemorySemanticClass};
use serde_json::{Value, json};

pub(super) fn cited_evidence_refs(value: &Value) -> BTreeSet<String> {
    value
        .get("because")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|reason| reason.get("ref").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect()
}

pub(super) fn wake_evidence_refs(value: &Value) -> BTreeSet<String> {
    value
        .pointer("/wake/causal_spine")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|claim| claim.get("evidence_ref").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect()
}

pub(super) fn rebuild_answer(value: &mut Value) {
    let citations = value
        .get("because")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|reason| {
            let evidence_ref = reason.get("ref").and_then(Value::as_str)?.trim();
            let claim = reason
                .get("claim")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            Some(if claim.is_empty() {
                evidence_ref.to_string()
            } else {
                format!("{claim} [{evidence_ref}]")
            })
        })
        .collect::<Vec<_>>();
    value["answer"] = match citations.as_slice() {
        [] => Value::Null,
        [single] => json!(format!(
            "Memory answer supported by {single}; canonical text is in proof.evidence."
        )),
        many => json!(format!(
            "Retrieved for this question by term overlap; read proof.evidence and judge whether it answers:\n{}",
            many.iter()
                .map(|citation| format!("- {citation}"))
                .collect::<Vec<_>>()
                .join("\n")
        )),
    };
}

pub(super) fn normalized_proof_relation(
    relation: &MemoryRelation,
    evidence: &[MemoryEvidence],
) -> MemoryRelation {
    let mut relation = relation.clone();
    let mut refs = relation
        .evidence_refs
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut repeated_why = false;
    let mut repeated_evidence = false;
    for item in evidence {
        let evidence_node_ref = item.id.strip_prefix("detail:").unwrap_or(&item.id);
        let incident = relation.source_ref == evidence_node_ref
            || relation.target_ref == evidence_node_ref
            || item.supports.iter().any(|supported_ref| {
                relation.source_ref == *supported_ref || relation.target_ref == *supported_ref
            });
        let why_matches = !relation.why.is_empty() && relation.why == item.text;
        let evidence_matches = !relation.evidence.is_empty() && relation.evidence == item.text;
        if incident || why_matches || evidence_matches {
            refs.insert(item.id.clone());
        }
        repeated_why |= why_matches;
        repeated_evidence |= evidence_matches;
    }
    if repeated_why {
        relation.why.clear();
    }
    if repeated_evidence {
        relation.evidence.clear();
    }
    relation.evidence_refs = refs.into_iter().collect();
    if relation.semantic_class != MemorySemanticClass::Structural as i32
        && relation.why.is_empty()
        && relation.evidence.is_empty()
        && !relation.evidence_refs.is_empty()
    {
        relation.why = "Supported by canonical evidence refs.".to_string();
    }
    relation
}

pub(super) fn normalized_answer_reason(
    reason: &AnswerReason,
    evidence: &[MemoryEvidence],
) -> AnswerReason {
    let mut reason = reason.clone();
    if evidence.iter().any(|item| {
        item.id == reason.r#ref && !item.text.is_empty() && item.text == reason.evidence
    }) {
        reason.evidence.clear();
    }
    reason
}

pub(super) fn normalized_ask_answer(
    answer: &str,
    reasons: &[AnswerReason],
    evidence: &[MemoryEvidence],
) -> String {
    let repeats_canonical_body = evidence.iter().any(|item| {
        let body = item.text.trim();
        !body.is_empty() && answer.contains(body)
    });
    if !repeats_canonical_body {
        return answer.to_string();
    }
    let mut seen = BTreeSet::new();
    let citations = reasons
        .iter()
        .filter_map(|reason| {
            let evidence_ref = reason.r#ref.trim();
            if evidence_ref.is_empty() || !seen.insert(evidence_ref.to_string()) {
                return None;
            }
            let claim = reason.claim.trim();
            Some(if claim.is_empty() {
                evidence_ref.to_string()
            } else {
                format!("{claim} [{evidence_ref}]")
            })
        })
        .collect::<Vec<_>>();
    match citations.as_slice() {
        [] => String::new(),
        [single] => format!(
            "Retrieved for this question by term overlap; read proof.evidence and judge whether \
             it answers: {single}"
        ),
        many => format!(
            "Retrieved for this question by term overlap; read proof.evidence and judge whether it answers:\n{}",
            many.iter()
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}
