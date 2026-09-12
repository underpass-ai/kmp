//! The deterministic recall packets and helpers the projection tests share.

use std::collections::BTreeSet;

use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use kmp_proto::v1beta1::{
    AnswerReason, AskResponse, MemoryConfidence, MemoryEvidence, MemoryLabel, MemoryRelation,
    MemorySemanticClass, WakeClaim, WakeResponse,
};
use prost_types::Timestamp;
use serde_json::{Value, json};

use super::projection_outcome::ProjectionOutcome;
use super::recall_output::project_recall_output;

impl ProjectionOutcome {
    pub(super) fn projected(self) -> Value {
        match self {
            Self::Projected(value) => value,
            Self::CoreTooLarge => panic!("fixture core should fit"),
        }
    }
}

pub(super) fn fixture() -> Value {
    json!({
        "summary": "Deterministic answer.",
        "answer": "Memory answer supported by claim:a [evidence:a]; canonical text is in proof.evidence.",
        "because": [{"claim": "claim:a", "ref": "evidence:a"}],
        "proof": {
            "path": [
                {"from": "claim:a", "to": "claim:b", "rel": "depends_on", "class": "causal", "why": "semantic"},
                {"from": "evidence:a", "to": "claim:a", "rel": "supports", "class": "evidential", "why": "support"},
                {"from": "root", "to": "claim:a", "rel": "contains_entry", "class": "structural"}
            ],
            "evidence": [
                {"id": "evidence:a", "supports": ["claim:a"], "text": "canonical body", "source": "test"},
                {"id": "evidence:b", "supports": ["claim:b"], "text": "additional body", "source": "test"}
            ],
            "conflicts": [],
            "superseded": [],
            "missing": ["raw:detail"],
            "frontier_size": 0,
            "matched_terms": [],
            "matched_relations": [],
            "confidence": "high"
        },
        "warnings": []
    })
}

pub(super) fn large_fixture(path_count: usize) -> Value {
    let reasons = (0..3)
        .map(|index| {
            json!({
                "claim": format!("claim:{index}"),
                "ref": format!("evidence:{index}")
            })
        })
        .collect::<Vec<_>>();
    let evidence = (0..8)
        .map(|index| {
            json!({
                "id": format!("evidence:{index}"),
                "supports": [format!("claim:{index}")],
                "text": format!("Canonical storage evidence {index}: {}", "grounded detail ".repeat(8)),
                "source": format!("source:{index}")
            })
        })
        .collect::<Vec<_>>();
    let path = (0..path_count)
        .map(|index| {
            let (rel, class) = match index % 3 {
                0 => ("depends_on", "causal"),
                1 => ("supports", "evidential"),
                _ => ("contains_entry", "structural"),
            };
            json!({
                "from": format!("node:{index:04}"),
                "to": format!("claim:{}", index % 3),
                "rel": rel,
                "class": class,
                "why": format!("Deterministic relation explanation {index}")
            })
        })
        .collect::<Vec<_>>();
    json!({
        "summary": "Deterministic memory answer from 3 evidence items.",
        "answer": "Retrieved for this question by term overlap; read proof.evidence and judge whether it answers:\n- claim:0 [evidence:0]\n- claim:1 [evidence:1]\n- claim:2 [evidence:2]",
        "because": reasons,
        "proof": {
            "path": path,
            "evidence": evidence,
            "conflicts": [],
            "superseded": [],
            "missing": ["raw:one", "raw:two"],
            "frontier_size": 2,
            "matched_terms": ["storage", "current"],
            "matched_relations": ["supports"],
            "confidence": "high"
        },
        "warnings": []
    })
}

pub(super) fn typed_ask_fixture(path_count: usize) -> AskResponse {
    let evidence = (0..8)
        .map(|index| MemoryEvidence {
            support_clocks: None,
            id: format!("evidence:{index}"),
            supports: vec![format!("claim:{index}")],
            text: format!(
                "Canonical storage evidence {index}: {}",
                "grounded detail ".repeat(8)
            ),
            source: format!("source:{index}"),
            time: None,
            metadata: Default::default(),
        })
        .collect::<Vec<_>>();
    let path = (0..path_count)
        .map(|index| {
            let (rel, semantic_class) = match index % 3 {
                0 => ("depends_on", MemorySemanticClass::Causal),
                1 => ("supports", MemorySemanticClass::Evidential),
                _ => ("contains_entry", MemorySemanticClass::Structural),
            };
            MemoryRelation {
                source_ref: format!("node:{index:04}"),
                target_ref: format!("claim:{}", index % 3),
                rel: rel.to_string(),
                semantic_class: semantic_class as i32,
                why: format!("Deterministic relation explanation {index}"),
                evidence: String::new(),
                confidence: MemoryConfidence::High as i32,
                sequence: None,
                explanation: None,
                evidence_refs: Vec::new(),
            }
        })
        .collect();
    let because = (0..3)
        .map(|index| AnswerReason {
            claim: format!("claim:{index}"),
            evidence: String::new(),
            r#ref: format!("evidence:{index}"),
        })
        .collect();
    AskResponse {
        summary: "Deterministic memory answer from 3 evidence items.".to_string(),
        answer: "This legacy prose is normalized from typed citations.".to_string(),
        because,
        proof: Some(kmp_proto::v1beta1::Proof {
            path,
            evidence,
            conflicts: Vec::new(),
            missing: vec!["raw:one".to_string(), "raw:two".to_string()],
            confidence: MemoryConfidence::High as i32,
            superseded: Vec::new(),
            expired: Vec::new(),
            frontier_size: 2,
            matched_terms: vec!["storage".to_string(), "current".to_string()],
            matched_relations: vec!["supports".to_string()],
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
    }
}

pub(super) fn typed_wake_fixture(path_count: usize) -> WakeResponse {
    let proof = typed_ask_fixture(path_count)
        .proof
        .expect("typed ask fixture proof");
    WakeResponse {
        dimension_selection: None,
        summary: "Deterministic wake packet.".to_string(),
        labels: Vec::new(),
        wake: Some(kmp_proto::v1beta1::WakePacket {
            objective: "continue parity work".to_string(),
            current_state: (0..8)
                .map(|index| format!("Current state {index}"))
                .collect(),
            causal_spine: vec![WakeClaim {
                claim: "claim:0".to_string(),
                because: "The canonical evidence supports it.".to_string(),
                evidence_ref: "evidence:0".to_string(),
            }],
            open_loops: (0..4).map(|index| format!("Open loop {index}")).collect(),
            next_actions: (0..4).map(|index| format!("Next action {index}")).collect(),
            guardrails: (0..4).map(|index| format!("Guardrail {index}")).collect(),
        }),
        proof: Some(proof),
        warnings: Vec::new(),
        resume_cursor: None,
        projection: None,
        truncation: None,
    }
}

pub(super) fn labels_fixture(count: usize) -> Vec<MemoryLabel> {
    (0..count)
        .map(|index| MemoryLabel {
            about: "project:kmp".to_string(),
            key: "task".to_string(),
            value: format!("underpass-ai-kmp-{index:03}"),
            entries: u32::try_from(count - index).expect("small count"),
            last_observed_at: Some(Timestamp {
                seconds: 1_756_000_000 + i64::try_from(index).expect("small index"),
                nanos: 0,
            }),
        })
        .collect()
}

pub(super) fn wake_request_with_bytes(max_bytes: u32) -> kmp_proto::v1beta1::WakeRequest {
    kmp_proto::v1beta1::WakeRequest {
        about: "project:kmp".to_string(),
        budget: Some(kmp_proto::v1beta1::MemoryBudget {
            max_bytes: u64::from(max_bytes),
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub(super) fn projected(packet: Value, arguments: Value) -> Value {
    project_recall_output(packet, &arguments, 2_400, &Cl100kEstimator::new())
        .expect("projection")
        .projected()
}

pub(super) fn relation_set(value: &Value) -> BTreeSet<String> {
    value
        .pointer("/proof/path")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|relation| relation.get("rel").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect()
}

pub(super) fn relation_values(value: &Value) -> BTreeSet<String> {
    value
        .pointer("/proof/path")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|relation| serde_json::to_string(relation).expect("relation"))
        .collect()
}

pub(super) fn evidence_set(value: &Value) -> BTreeSet<String> {
    string_set(value, "/proof/evidence")
}

pub(super) fn string_set(value: &Value, pointer: &str) -> BTreeSet<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| serde_json::to_string(item).expect("projection item"))
        .collect()
}
