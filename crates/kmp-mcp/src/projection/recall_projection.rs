use serde_json::Value;

use kmp_proto::v1beta1::{AskResponse, WakeResponse};

use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use kmp_domain::TokenEstimator;
use kmp_proto_mapping::v1beta1::recall_projection::{ProjectionOutcome, project_recall_output};

use crate::serving::tool_error::ToolError;

pub(crate) fn wake_from_response(response: WakeResponse) -> Value {
    kmp_proto_mapping::v1beta1::recall_projection::wake_value(&response)
}

pub(crate) fn ask_from_response(response: AskResponse) -> Value {
    kmp_proto_mapping::v1beta1::recall_projection::ask_value(&response)
}

/// Applies the caller's budget to the JSON that the MCP host actually sees.
///
/// The application renderer ranks/selects memory and budgets its prose. This
/// host gateway then preserves the cited core and adds a fixed, pageable
/// expansion prefix under a normative serialized-byte ceiling.
#[cfg(test)]
pub(crate) fn enforce_recall_output_budget(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
) -> Value {
    let estimator = Cl100kEstimator::new();
    enforce_recall_output_budget_with_estimator(value, arguments, default_tokens, &estimator)
}

#[cfg(test)]
fn enforce_recall_output_budget_with_estimator(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
    estimator: &dyn TokenEstimator,
) -> Value {
    try_enforce_recall_output_budget_with_estimator(value, arguments, default_tokens, estimator)
        .expect("test recall cursor should be valid")
}

pub(crate) fn try_enforce_recall_output_budget(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
) -> Result<Value, ToolError> {
    let estimator = Cl100kEstimator::new();
    try_enforce_recall_output_budget_with_estimator(value, arguments, default_tokens, &estimator)
}

fn try_enforce_recall_output_budget_with_estimator(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
    estimator: &dyn TokenEstimator,
) -> Result<Value, ToolError> {
    match project_recall_output(value, arguments, default_tokens, estimator)? {
        ProjectionOutcome::Projected(value) => Ok(value),
        ProjectionOutcome::CoreTooLarge => Err(ToolError::invalid_argument(
            "recall projection byte budget is smaller than the stable citation core; raise \
             budget.max_bytes",
        )),
    }
}

#[cfg(test)]
pub(crate) fn array_len(value: &Value, path: &[&str]) -> usize {
    let mut current = value;
    for key in path {
        let Some(next) = current.get(*key) else {
            return 0;
        };
        current = next;
    }
    current.as_array().map(Vec::len).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::rendering::{memory_evidence_json, memory_relation_json};
    use crate::projection::test_support::fixtures::*;
    #[allow(unused_imports)]
    use kmp_proto::v1beta1::*;
    #[allow(unused_imports)]
    use serde_json::{Value, json};

    #[test]
    fn maps_typed_ask_response_without_inventing_null_answer() {
        let response = AskResponse {
            summary: "No deterministic memory answer found.".to_string(),
            answer: String::new(),
            because: vec![AnswerReason {
                claim: "claim".to_string(),
                evidence: "evidence".to_string(),
                r#ref: "evidence:1".to_string(),
            }],
            proof: Some(Proof {
                path: vec![relation()],
                evidence: vec![evidence()],
                conflicts: Vec::new(),
                superseded: Vec::new(),
                expired: Vec::new(),
                missing: vec!["generative_answer".to_string()],
                frontier_size: 1,
                matched_terms: vec!["deterministic".to_string()],
                matched_relations: vec!["supports".to_string()],
                confidence: MemoryConfidence::Medium as i32,
                interval: None,
                axis: 0,
                nearest_outside: None,
                as_of: None,
                abouts_selected: Vec::new(),
                abouts_empty_in_selection: Vec::new(),
            }),
            warnings: Vec::new(),
            projection: None,
            truncation: None,
            asked_as: String::new(),
        };

        let value = ask_from_response(response);

        assert_eq!(value["answer"], Value::Null);
        assert_eq!(value["because"][0]["ref"], "evidence:1");
        assert_eq!(value["proof"]["path"][0]["from"], "claim:source");
        assert_eq!(value["proof"]["confidence"], "medium");
        assert_eq!(value["proof"]["frontier_size"], 1);
    }
    #[test]
    fn three_reason_packet_serializes_each_evidence_body_once() {
        let bodies = (0..3)
            .map(|index| {
                format!(
                    "Evidence body {index} establishes the selected claim with exact temporal and provenance detail. {}",
                    "grounded context remains canonical here. ".repeat(24)
                )
            })
            .collect::<Vec<_>>();
        let reasons = bodies
            .iter()
            .enumerate()
            .map(|(index, body)| AnswerReason {
                claim: format!("claim:{index}"),
                evidence: body.clone(),
                r#ref: format!("detail:evidence:{index}"),
            })
            .collect::<Vec<_>>();
        let evidence = bodies
            .iter()
            .enumerate()
            .map(|(index, body)| MemoryEvidence {
                id: format!("detail:evidence:{index}"),
                supports: vec![format!("claim:{index}")],
                text: body.clone(),
                source: format!("source:{index}"),
                time: None,
                metadata: Default::default(),
            })
            .collect::<Vec<_>>();
        let path = (0..12)
            .map(|index| {
                let evidence_index = index % 3;
                MemoryRelation {
                    source_ref: format!("evidence:{evidence_index}"),
                    target_ref: format!("claim:{evidence_index}"),
                    rel: "supports".to_string(),
                    semantic_class: MemorySemanticClass::Evidential as i32,
                    why: "Evidence supports the selected memory claim.".to_string(),
                    evidence: bodies[evidence_index].clone(),
                    confidence: MemoryConfidence::High as i32,
                    sequence: Some(index as u32 + 1),
                    explanation: None,
                    evidence_refs: Vec::new(),
                }
            })
            .collect::<Vec<_>>();
        let legacy_answer = bodies
            .iter()
            .map(|body| format!("- {body}"))
            .collect::<Vec<_>>()
            .join("\n");
        let response = AskResponse {
            summary: "Deterministic memory answer from 3 evidence items.".to_string(),
            answer: legacy_answer.clone(),
            because: reasons.clone(),
            proof: Some(Proof {
                path: path.clone(),
                evidence: evidence.clone(),
                conflicts: Vec::new(),
                superseded: Vec::new(),
                expired: Vec::new(),
                missing: Vec::new(),
                frontier_size: 0,
                matched_terms: vec!["canonical".to_string()],
                matched_relations: vec!["supports".to_string()],
                confidence: MemoryConfidence::High as i32,
                interval: None,
                axis: 0,
                nearest_outside: None,
                as_of: None,
                abouts_selected: Vec::new(),
                abouts_empty_in_selection: Vec::new(),
            }),
            warnings: Vec::new(),
            projection: None,
            truncation: None,
            asked_as: String::new(),
        };
        let legacy = json!({
            "summary": response.summary,
            "answer": legacy_answer,
            "because": reasons.iter().map(|reason| json!({
                "claim": reason.claim,
                "evidence": reason.evidence,
                "ref": reason.r#ref
            })).collect::<Vec<_>>(),
            "proof": {
                "path": path.iter().map(memory_relation_json).collect::<Vec<_>>(),
                "evidence": evidence.iter().map(memory_evidence_json).collect::<Vec<_>>(),
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "matched_terms": ["canonical"],
                "matched_relations": ["supports"],
                "confidence": "high"
            },
            "warnings": []
        });

        let outputs = (0..3)
            .map(|_| {
                serde_json::to_string(&ask_from_response(response.clone()))
                    .expect("normalized ask should serialize")
            })
            .collect::<Vec<_>>();
        let normalized = &outputs[0];
        let legacy = serde_json::to_string(&legacy).expect("legacy ask should serialize");
        let estimator = Cl100kEstimator::new();
        let normalized_tokens = estimator.estimate_tokens(normalized);
        let legacy_tokens = estimator.estimate_tokens(&legacy);
        let normalized_value: Value =
            serde_json::from_str(normalized).expect("normalized ask should parse");
        let legacy_value: Value = serde_json::from_str(&legacy).expect("legacy ask should parse");
        let canonical_body_bytes = bodies.iter().map(String::len).sum::<usize>();
        // Legacy owns every body in answer, because, proof.evidence, and four
        // supporting path hops. The normalized packet owns the registry copy.
        let legacy_owned_body_bytes = canonical_body_bytes * 7;
        let normalized_owned_body_bytes = canonical_body_bytes;
        let samples = 250;
        let legacy_started = std::time::Instant::now();
        for _ in 0..samples {
            std::hint::black_box(
                serde_json::to_vec(std::hint::black_box(&legacy_value))
                    .expect("legacy ask should serialize repeatedly"),
            );
        }
        let legacy_serialization = legacy_started.elapsed();
        let normalized_started = std::time::Instant::now();
        for _ in 0..samples {
            std::hint::black_box(
                serde_json::to_vec(std::hint::black_box(&normalized_value))
                    .expect("normalized ask should serialize repeatedly"),
            );
        }
        let normalized_serialization = normalized_started.elapsed();

        println!(
            "three-reason normalization: {} -> {} bytes; {} -> {} advisory cl100k tokens; owned evidence-body bytes {} -> {}; {samples} serializations {:?} -> {:?}",
            legacy.len(),
            normalized.len(),
            legacy_tokens,
            normalized_tokens,
            legacy_owned_body_bytes,
            normalized_owned_body_bytes,
            legacy_serialization,
            normalized_serialization
        );

        assert!(outputs.windows(2).all(|pair| pair[0] == pair[1]));
        for body in &bodies {
            assert_eq!(normalized.matches(body).count(), 1, "{body}");
        }
        assert!(
            normalized.len() * 100 <= legacy.len() * 40,
            "normalized={} legacy={}",
            normalized.len(),
            legacy.len()
        );
        assert!(normalized.len() < 10_000, "{} bytes", normalized.len());

        let canonical_ids = normalized_value["proof"]["evidence"]
            .as_array()
            .expect("canonical evidence registry")
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(
            normalized_value["because"]
                .as_array()
                .expect("reasons")
                .iter()
                .all(|reason| reason.get("evidence").is_none()
                    && reason["ref"]
                        .as_str()
                        .is_some_and(|evidence_ref| canonical_ids.contains(evidence_ref)))
        );
        assert!(
            normalized_value["proof"]["path"]
                .as_array()
                .expect("proof path")
                .iter()
                .all(|relation| relation.get("evidence").is_none()
                    && relation["evidence_refs"]
                        .as_array()
                        .is_some_and(|refs| !refs.is_empty()
                            && refs.iter().all(|evidence_ref| {
                                evidence_ref.as_str().is_some_and(|evidence_ref| {
                                    canonical_ids.contains(evidence_ref)
                                })
                            })))
        );
    }
    #[test]
    fn maps_wake_and_ignores_transport_budget_types() {
        let response = WakeResponse {
            summary: "Wake summary".to_string(),
            labels: Vec::new(),
            wake: Some(WakePacket {
                objective: "continue".to_string(),
                current_state: vec!["state".to_string()],
                causal_spine: vec![WakeClaim {
                    claim: "claim".to_string(),
                    because: "because".to_string(),
                    evidence_ref: "evidence:1".to_string(),
                }],
                open_loops: Vec::new(),
                next_actions: Vec::new(),
                guardrails: Vec::new(),
            }),
            proof: None,
            resume_cursor: Some(TemporalCursor {
                r#ref: "decision:latest".to_string(),
                time: Some(prost_types::Timestamp {
                    seconds: 1_786_924_800,
                    nanos: 0,
                }),
                sequence: Some(3),
            }),
            warnings: Vec::new(),
            projection: None,
            truncation: None,
        };
        let _budget = MemoryBudget {
            tokens: 1,
            detail: MemoryDetailLevel::Full as i32,
            depth: 1,
            max_entries: 0,
            max_bytes: 0,
        };

        let value = wake_from_response(response);

        assert_eq!(value["wake"]["current_state"][0], "state");
        assert_eq!(
            value["wake"]["causal_spine"][0]["evidence_ref"],
            "evidence:1"
        );
        // The bookmark a caller carries to kmp_forward, so catching up is
        // one more call rather than a rewind that exists to find a timestamp.
        assert_eq!(value["resume_cursor"]["ref"], "decision:latest");
        assert_eq!(value["resume_cursor"]["sequence"], 3);
    }
}
