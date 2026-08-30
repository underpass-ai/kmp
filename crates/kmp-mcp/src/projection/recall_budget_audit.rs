//! The recall byte budget, audited end to end: sweeps that prove the
//! ceiling holds while the cited core stays monotone, and that trimming
//! never invents or loses graph structure.
#![cfg(test)]

mod tests {
    #[allow(unused_imports)]
    use crate::projection::recall_projection::*;
    use crate::projection::test_support::fixtures::*;
    #[allow(unused_imports)]
    use kmp_proto::v1beta1::*;
    #[allow(unused_imports)]
    use serde_json::{Value, json};

    /// #439: a ceiling below the stable floor returns the floor and says
    /// so, instead of an error whose only answer was to over-budget. One
    /// call, never fails, spend = the floor — and the caller is told the
    /// number to raise.
    #[test]
    fn a_budget_below_the_stable_floor_returns_the_floor_and_says_so() {
        for (value, default_tokens) in [
            (recall_budget_fixture(), 2_400u32),
            (wake_budget_fixture(), 1_600u32),
        ] {
            for byte_limit in [512usize, 640, 768] {
                let bounded = enforce_recall_output_budget(
                    value.clone(),
                    &json!({"budget": {"max_bytes": byte_limit, "detail": "balanced"}}),
                    default_tokens,
                );
                let warnings = bounded["warnings"].as_array().expect("warnings channel");
                assert!(
                    warnings
                        .iter()
                        .any(|warning| warning.as_str().is_some_and(|text| {
                            text.contains("stable floor") && text.contains(&byte_limit.to_string())
                        })),
                    "the floor must say why it exceeds max_bytes {byte_limit}: {warnings:?}"
                );
                let size = serde_json::to_vec(&bounded).expect("serialize").len();
                assert!(
                    size < 4_000,
                    "the floor is minimal, not a budget escape hatch: {size} bytes"
                );
            }
        }
    }

    #[test]
    fn compact_output_filters_structure_and_honours_the_serialized_byte_limit() {
        let path = (0..40)
            .map(|index| {
                json!({
                    "from": format!("about:root:{index}"),
                    "to": format!("entry:{index}"),
                    "rel": "contains_entry",
                    "class": "structural",
                    "why": "Repeated structural boilerplate that must not fill compact output",
                    "confidence": "high"
                })
            })
            .collect::<Vec<_>>();
        let evidence = (0..20)
            .map(|index| {
                json!({
                    "id": format!("evidence:{index}"),
                    "supports": [format!("entry:{index}")],
                    "text": "Long evidence text repeated to exercise final MCP packet budgeting",
                    "source": format!("source:{index}")
                })
            })
            .collect::<Vec<_>>();
        let value = json!({
            "summary": "Recall summary",
            "wake": {"current_state": ["state"], "causal_spine": []},
            "proof": {
                "path": path,
                "evidence": evidence,
                "missing": [],
                "frontier_size": 0,
                "confidence": "medium"
            },
            "warnings": []
        });
        let arguments = json!({"budget": {"tokens": 1, "max_bytes": 2_000, "detail": "compact"}});

        let bounded = enforce_recall_output_budget(value, &arguments, 1600);
        assert!(
            serde_json::to_vec(&bounded)
                .expect("bounded projection")
                .len()
                <= 2_000
        );
        assert!(
            bounded["proof"]["path"]
                .as_array()
                .expect("proof path remains typed")
                .iter()
                .all(|relation| relation["class"] != "structural")
        );
        assert_eq!(bounded["truncation"]["truncated"], true);
    }
    #[test]
    fn transport_omissions_do_not_change_the_graph_frontier() {
        let value = json!({
            "summary": "answer",
            "answer": "UNKNOWN",
            "because": [],
            "proof": {
                "path": [
                    {"from": "root", "to": "claim:one", "rel": "contains_entry", "class": "structural"},
                    {"from": "root", "to": "claim:two", "rel": "contains_entry", "class": "structural"}
                ],
                "evidence": [],
                "conflicts": [],
                "superseded": [],
                "missing": ["unexplored:one", "unexplored:two"],
                "frontier_size": 2,
                "confidence": "high"
            },
            "warnings": []
        });
        let low_budget = enforce_recall_output_budget(
            value.clone(),
            &json!({"budget": {"tokens": 500, "detail": "compact"}}),
            2_400,
        );
        let high_budget = enforce_recall_output_budget(
            value,
            &json!({"budget": {"tokens": 1_200, "detail": "full"}}),
            2_400,
        );

        for result in [&low_budget, &high_budget] {
            assert_eq!(result["proof"]["frontier_size"], 2);
        }
        assert_eq!(low_budget["truncation"]["truncated"], true);
        assert!(high_budget.get("truncation").is_none());
        assert_eq!(low_budget["projection"]["excluded_by_detail"], 4);
        assert_eq!(high_budget["projection"]["excluded_by_detail"], 0);
    }
    #[test]
    fn max_entries_caps_ask_reasons_as_well_as_proof_evidence() {
        let reasons = (0..5)
            .map(|index| json!({"evidence": format!("answer {index}")}))
            .collect::<Vec<_>>();
        let evidence = (0..5)
            .map(|index| json!({"id": format!("evidence:{index}"), "text": "answer"}))
            .collect::<Vec<_>>();
        let value = json!({
            "summary": "answer",
            "answer": "unbounded",
            "because": reasons,
            "proof": {"path": [], "evidence": evidence, "missing": [], "frontier_size": 0},
            "warnings": []
        });

        let bounded = enforce_recall_output_budget(
            value,
            &json!({"budget": {"tokens": 1000, "max_entries": 2}}),
            2400,
        );

        assert_eq!(bounded["because"].as_array().expect("reasons").len(), 2);
        assert_eq!(
            bounded["proof"]["evidence"]
                .as_array()
                .expect("evidence")
                .len(),
            2
        );
    }
    #[test]
    fn projection_envelope_is_budgeted_without_losing_the_cited_core() {
        let value = recall_budget_fixture();
        let mut compact_source = value.clone();
        compact_source["proof"]["path"]
            .as_array_mut()
            .expect("proof path")
            .retain(|relation| relation["class"] != "structural");
        let byte_limit = serde_json::to_vec(&compact_source)
            .expect("compact source should serialize")
            .len();

        // This is the exact cliff from #94. The projection envelope is now
        // reserved before filling the fixed expansion prefix.
        let bounded = enforce_recall_output_budget(
            value,
            &json!({"budget": {"tokens": 1, "max_bytes": byte_limit, "detail": "compact"}}),
            2_400,
        );

        assert!(
            serde_json::to_vec(&bounded)
                .expect("bounded projection")
                .len()
                <= byte_limit
        );
        assert_eq!(bounded["because"].as_array().expect("reasons").len(), 3);
        assert_eq!(bounded["truncation"]["truncated"], true);
        assert_eq!(
            bounded["projection"]["contract"],
            "kmp.recall.projection.v1"
        );
        assert!(!bounded["warnings"].as_array().expect("warnings").is_empty());
    }
    #[test]
    fn ask_budget_sweep_preserves_the_ceiling_and_monotone_cited_core() {
        let value = recall_budget_fixture();
        for detail in ["compact", "balanced", "full"] {
            let mut previous_reasons = 0;
            let mut previous_evidence = 0;
            for byte_limit in (4_000..=10_000).step_by(100) {
                let bounded = enforce_recall_output_budget(
                    value.clone(),
                    &json!({
                        "budget": {
                            "tokens": 1,
                            "max_bytes": byte_limit,
                            "detail": detail,
                            "max_entries": 12
                        }
                    }),
                    2_400,
                );
                let reasons = array_len(&bounded, &["because"]);
                let evidence = array_len(&bounded, &["proof", "evidence"]);

                assert!(
                    serde_json::to_vec(&bounded)
                        .expect("projection should serialize")
                        .len()
                        <= byte_limit,
                    "{detail} exceeded the normative byte ceiling {byte_limit}"
                );
                assert!(
                    reasons >= previous_reasons,
                    "{detail} lost cited reasons when the byte budget grew to {byte_limit}"
                );
                assert!(
                    evidence >= previous_evidence,
                    "{detail} lost proof evidence when the byte budget grew to {byte_limit}"
                );
                if bounded
                    .pointer("/truncation/truncated")
                    .and_then(Value::as_bool)
                    == Some(true)
                {
                    assert_eq!(
                        bounded["projection"]["contract"],
                        "kmp.recall.projection.v1"
                    );
                    assert!(!bounded["warnings"].as_array().expect("warnings").is_empty());
                }
                previous_reasons = reasons;
                previous_evidence = evidence;
            }
        }
    }
    #[test]
    fn wake_budget_sweep_preserves_the_ceiling_and_monotone_state() {
        let value = wake_budget_fixture();
        let mut previous_state = 0;

        for byte_limit in (4_000..=10_000).step_by(100) {
            let bounded = enforce_recall_output_budget(
                value.clone(),
                &json!({"budget": {"tokens": 1, "max_bytes": byte_limit, "detail": "balanced"}}),
                2_400,
            );
            let retained_state = [
                "current_state",
                "causal_spine",
                "open_loops",
                "next_actions",
                "guardrails",
            ]
            .into_iter()
            .map(|section| array_len(&bounded, &["wake", section]))
            .sum::<usize>();

            assert!(
                serde_json::to_vec(&bounded)
                    .expect("projection should serialize")
                    .len()
                    <= byte_limit
            );
            assert!(
                retained_state >= previous_state,
                "wake lost state when the byte budget grew to {byte_limit}"
            );
            if bounded
                .pointer("/truncation/truncated")
                .and_then(Value::as_bool)
                == Some(true)
            {
                assert_eq!(
                    bounded["projection"]["contract"],
                    "kmp.recall.projection.v1"
                );
                assert!(!bounded["warnings"].as_array().expect("warnings").is_empty());
            }
            previous_state = retained_state;
        }
    }
    #[test]
    fn ask_budget_shortens_text_without_dropping_stable_citations() {
        let reasons = (0..5)
            .map(|index| {
                json!({
                    "claim": format!("claim:{index}"),
                    "evidence": format!("reason {index} {}", "evidence ".repeat(4_000)),
                    "ref": format!("evidence:{index}")
                })
            })
            .collect::<Vec<_>>();
        let answer = reasons
            .iter()
            .map(|reason| format!("- {}", reason["evidence"].as_str().expect("evidence")))
            .collect::<Vec<_>>()
            .join("\n");
        let value = json!({
            "summary": "Deterministic memory answer from 5 evidence items for: why?",
            "answer": answer,
            "because": reasons,
            "proof": {
                "path": [],
                "evidence": [],
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "confidence": "high"
            },
            "warnings": []
        });

        for arguments in [
            json!({}),
            json!({"budget": {"detail": "compact"}}),
            json!({"budget": {"tokens": 400}}),
        ] {
            let bounded = enforce_recall_output_budget(value.clone(), &arguments, 2_400);

            assert!(
                serde_json::to_vec(&bounded)
                    .expect("projection should serialize")
                    .len()
                    <= 10_000
            );
            assert!(
                bounded["answer"]
                    .as_str()
                    .is_some_and(|answer| !answer.is_empty()),
                "the payload must survive the budget gate"
            );
            assert_eq!(bounded["because"].as_array().expect("citations").len(), 5);
            assert!(
                bounded["because"]
                    .as_array()
                    .expect("citations")
                    .iter()
                    .enumerate()
                    .all(|(index, reason)| reason["ref"] == format!("evidence:{index}"))
            );
            assert!(bounded["proof"].is_object());
            assert_eq!(bounded["proof"]["confidence"], "high");
            assert_eq!(bounded["truncation"]["truncated"], true);
            assert!(bounded["truncation"]["omitted"].is_object());
            assert_eq!(bounded["projection"]["core_text_shortened"], true);
            assert!(bounded["truncation"].get("omitted_items").is_none());
            assert!(
                bounded["proof"]["missing"]
                    .as_array()
                    .expect("missing")
                    .is_empty()
            );
        }
    }
    #[test]
    fn wake_budget_keeps_the_wake_shape_when_retained_text_is_too_large() {
        let value = json!({
            "summary": "Objective: keep working.",
            "wake": {
                "objective": format!("objective {}", "detail ".repeat(4_000)),
                "current_state": [format!("state {}", "detail ".repeat(4_000))],
                "causal_spine": [{
                    "claim": "claim",
                    "because": format!("because {}", "detail ".repeat(4_000)),
                    "evidence_ref": "evidence:1"
                }],
                "open_loops": [],
                "next_actions": [],
                "guardrails": []
            },
            "proof": {
                "path": [],
                "evidence": [],
                "conflicts": [],
                "superseded": [],
                "missing": [],
                "frontier_size": 0,
                "confidence": "medium"
            },
            "resume_cursor": {"ref": "decision:latest"},
            "warnings": []
        });

        let bounded = enforce_recall_output_budget(value, &json!({}), 1_600);
        assert!(serde_json::to_vec(&bounded).expect("bounded wake").len() <= 10_000);
        assert!(bounded["wake"].is_object());
        assert!(bounded["proof"].is_object());
        assert_eq!(bounded["resume_cursor"]["ref"], "decision:latest");
        assert_eq!(bounded["truncation"]["truncated"], true);
        assert_eq!(bounded["projection"]["core_text_shortened"], true);
        assert_eq!(
            bounded["wake"]["causal_spine"][0]["evidence_ref"],
            "evidence:1"
        );
    }
}
