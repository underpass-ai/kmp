//! The fixtures the projection tests share: a recall bundle with the
//! relations, evidence and coordinates a budget sweep needs, and a byte
//! counter that measures what a host would actually receive.
#![cfg(test)]

pub(crate) mod fixtures {
    #[allow(unused_imports)]
    use kmp_proto::v1beta1::*;
    #[allow(unused_imports)]
    use serde_json::{Value, json};

    pub(crate) fn byte_len(value: &serde_json::Value) -> usize {
        serde_json::to_string(value).expect("serializes").len()
    }
    pub(crate) fn recall_budget_fixture() -> Value {
        let reasons = (0..3)
            .map(|index| {
                json!({
                    "claim": format!("claim:{index}"),
                    "evidence": format!("Reason {index}: {}", "specific evidence detail ".repeat(6)),
                    "ref": format!("evidence:{index}")
                })
            })
            .collect::<Vec<_>>();
        let answer = reasons
            .iter()
            .map(|reason| format!("- {}", reason["evidence"].as_str().expect("evidence")))
            .collect::<Vec<_>>()
            .join("\n");
        let mut path = vec![json!({
            "from": "about:root",
            "to": "claim:0",
            "rel": "contains_entry",
            "class": "structural",
            "why": "structural bookkeeping",
            "confidence": "high"
        })];
        path.extend((0..8).map(|index| {
            json!({
                "from": format!("claim:{index}"),
                "to": format!("evidence:{}", index % 3),
                "rel": "supports",
                "class": "evidential",
                "why": format!("Semantic proof hop {index}: {}", "relationship detail ".repeat(4)),
                "evidence": format!("hop evidence {index}: {}", "grounded detail ".repeat(4)),
                "confidence": "high"
            })
        }));
        let evidence = (0..3)
            .map(|index| {
                json!({
                    "id": format!("evidence:{index}"),
                    "supports": [format!("claim:{index}")],
                    "text": format!("Reason {index}: {}", "specific evidence detail ".repeat(6)),
                    "source": format!("source:{index}")
                })
            })
            .collect::<Vec<_>>();

        json!({
            "summary": "Deterministic memory answer from 3 evidence items.",
            "answer": answer,
            "because": reasons,
            "proof": {
                "path": path,
                "evidence": evidence,
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "matched_terms": ["truncation"],
                "matched_relations": ["supports"],
                "confidence": "high"
            },
            "warnings": []
        })
    }
    pub(crate) fn wake_budget_fixture() -> Value {
        let state = (0..6)
            .map(|index| format!("state {index}: {}", "specific retained detail ".repeat(4)))
            .collect::<Vec<_>>();
        let claims = (0..6)
            .map(|index| {
                json!({
                    "claim": format!("claim {index}: {}", "specific retained detail ".repeat(4)),
                    "because": format!("reason {index}: {}", "grounded detail ".repeat(4)),
                    "evidence_ref": format!("evidence:{index}")
                })
            })
            .collect::<Vec<_>>();

        json!({
            "summary": "Wake summary",
            "wake": {
                "objective": "continue truncation work",
                "current_state": state,
                "causal_spine": claims,
                "open_loops": ["open loop 0", "open loop 1", "open loop 2"],
                "next_actions": ["next action 0", "next action 1", "next action 2"],
                "guardrails": ["guardrail 0", "guardrail 1", "guardrail 2"]
            },
            "proof": {
                "path": [],
                "evidence": [],
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "confidence": "high"
            },
            "resume_cursor": {"ref": "decision:latest"},
            "warnings": []
        })
    }
    pub(crate) fn relation() -> MemoryRelation {
        MemoryRelation {
            source_ref: "claim:source".to_string(),
            target_ref: "claim:target".to_string(),
            rel: "supports".to_string(),
            semantic_class: MemorySemanticClass::Evidential as i32,
            why: "why".to_string(),
            evidence: "evidence".to_string(),
            confidence: MemoryConfidence::High as i32,
            sequence: Some(1),
            explanation: None,
            evidence_refs: Vec::new(),
        }
    }
    pub(crate) fn evidence() -> MemoryEvidence {
        MemoryEvidence {
            id: "evidence:1".to_string(),
            supports: vec!["claim:target".to_string()],
            text: "Evidence".to_string(),
            source: "source".to_string(),
            time: None,
            metadata: Default::default(),
        }
    }
    pub(crate) fn coordinate() -> TemporalCoordinate {
        TemporalCoordinate {
            dimension: "timeline".to_string(),
            scope_id: "scope".to_string(),
            occurred_at: None,
            observed_at: None,
            ingested_at: None,
            valid_from: None,
            valid_until: None,
            sequence: Some(2),
            rank: None,
            metadata: Default::default(),
        }
    }
}
