//! What a continuation cursor is bound to, and what the pages it walks must
//! reconstruct.

use std::collections::BTreeSet;

use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use serde_json::{Value, json};

use super::recall_output::project_recall_output;
use super::test_support::{
    evidence_set, fixture, large_fixture, projected, relation_values, string_set,
};

#[test]
fn cursor_rejects_changed_bound_arguments() {
    let estimator = Cl100kEstimator::new();
    let packet = fixture();
    let first = project_recall_output(
        packet.clone(),
        &json!({
            "about": "project:kmp",
            "question": "What is current?",
            "budget": {"tokens": 900, "max_bytes": 4_000, "detail": "full"},
            "page": {"entries": 1}
        }),
        2_400,
        &estimator,
    )
    .expect("first page")
    .projected();
    let cursor = first
        .pointer("/projection/page/next_cursor")
        .and_then(Value::as_str)
        .expect("continuation cursor");
    let error = project_recall_output(
        packet,
        &json!({
            "about": "project:kmp",
            "question": "A changed question",
            "budget": {"tokens": 900, "max_bytes": 4_000, "detail": "full"},
            "page": {"entries": 1, "cursor": cursor}
        }),
        2_400,
        &estimator,
    )
    .expect_err("changed bound arguments must invalidate cursor");
    assert!(error.contains("does not match"));
}

#[test]
fn pages_reconstruct_the_full_proof_without_changing_the_answer() {
    let packet = large_fixture(21);
    let base_arguments = json!({
        "about": "project:kmp",
        "question": "Which storage engine is current?",
        "budget": {"tokens": 30_000, "max_bytes": 100_000, "detail": "full"}
    });
    let complete = projected(packet.clone(), base_arguments.clone());
    let expected_evidence = evidence_set(&complete);
    let expected_relations = relation_values(&complete);
    let expected_missing = string_set(&complete, "/proof/missing");
    let expected_answer = complete["answer"].clone();
    let expected_because = complete["because"].clone();
    let expected_core_evidence =
        complete["proof"]["evidence"].as_array().expect("evidence")[..3].to_vec();

    let mut cursor = None;
    let mut evidence = BTreeSet::new();
    let mut relations = BTreeSet::new();
    let mut missing = BTreeSet::new();
    let mut pages = 0usize;
    loop {
        let mut arguments = base_arguments.clone();
        arguments["page"] = json!({"entries": 4});
        if let Some(cursor) = &cursor {
            arguments["page"]["cursor"] = json!(cursor);
        }
        let page = projected(packet.clone(), arguments);
        assert_eq!(page["answer"], expected_answer);
        assert_eq!(page["because"], expected_because);
        assert!(
            page["proof"]["evidence"]
                .as_array()
                .expect("evidence")
                .starts_with(&expected_core_evidence)
        );
        evidence.extend(evidence_set(&page));
        relations.extend(relation_values(&page));
        missing.extend(string_set(&page, "/proof/missing"));
        pages += 1;

        if page["projection"]["page"]["has_more"] == false {
            assert!(page["projection"]["page"]["next_cursor"].is_null());
            assert_eq!(page["truncation"]["omitted"]["remaining_page_items"], 0);
            assert!(
                page["warnings"]
                    .as_array()
                    .expect("warnings")
                    .iter()
                    .any(|warning| warning
                        .as_str()
                        .is_some_and(|warning| warning.contains("final continuation page")))
            );
            break;
        }
        cursor = Some(
            page["projection"]["page"]["next_cursor"]
                .as_str()
                .expect("opaque cursor")
                .to_string(),
        );
        assert!(pages < 20, "cursor must make forward progress");
    }

    assert!(pages > 1);
    assert_eq!(evidence, expected_evidence);
    assert_eq!(relations, expected_relations);
    assert_eq!(missing, expected_missing);
}

#[test]
fn cursor_identity_uses_the_canonical_ordered_selection() {
    let packet = large_fixture(12);
    let mut reordered = packet.clone();
    reordered["proof"]["path"]
        .as_array_mut()
        .expect("proof path")
        .reverse();
    let base_arguments = json!({
        "about": "project:kmp",
        "question": "Which storage engine is current?",
        "budget": {"tokens": 30_000, "max_bytes": 100_000, "detail": "full"},
        "page": {"entries": 4}
    });
    let first = projected(packet, base_arguments.clone());
    let cursor = first["projection"]["page"]["next_cursor"]
        .as_str()
        .expect("continuation cursor");
    let mut continuation = base_arguments;
    continuation["page"]["cursor"] = json!(cursor);

    let second = projected(reordered, continuation);
    assert_eq!(second["projection"]["page"]["offset"], 4);
    assert!(second["projection"]["page"]["returned"].as_u64() > Some(0));
}
