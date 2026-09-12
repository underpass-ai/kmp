//! What the detail tiers and the advisory token hint are allowed to change.

use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use serde_json::{Value, json};

use super::recall_output::project_recall_output;
use super::test_support::{evidence_set, fixture, large_fixture, projected, relation_set};

#[test]
fn detail_modes_are_nested_fieldsets() {
    let packet = fixture();
    let args = |detail: &str| {
        json!({
            "about": "project:kmp",
            "question": "What is current?",
            "budget": {"tokens": 10_000, "max_bytes": 40_000, "detail": detail}
        })
    };
    let compact = projected(packet.clone(), args("compact"));
    let balanced = projected(packet.clone(), args("balanced"));
    let full = projected(packet, args("full"));

    assert_eq!(compact["because"], balanced["because"]);
    assert_eq!(balanced["because"], full["because"]);
    assert!(relation_set(&compact).is_subset(&relation_set(&balanced)));
    assert!(relation_set(&balanced).is_subset(&relation_set(&full)));
    assert!(evidence_set(&compact).is_subset(&evidence_set(&balanced)));
    assert!(evidence_set(&balanced).is_subset(&evidence_set(&full)));
    assert!(relation_set(&compact).contains("depends_on"));
    assert!(relation_set(&balanced).contains("supports"));
    assert!(relation_set(&full).contains("contains_entry"));
}

#[test]
fn detail_levels_change_expansion_when_bytes_are_available() {
    let packet = large_fixture(80);
    let args = |detail: &str| {
        json!({
            "about": "project:kmp",
            "question": "What is current?",
            // Deliberately omit `tokens`: its default is a compatibility
            // hint, not a hidden cap on otherwise available bytes.
            "budget": {"max_bytes": 60_000, "detail": detail}
        })
    };
    let compact = projected(packet.clone(), args("compact"));
    let balanced = projected(packet.clone(), args("balanced"));
    let full = projected(packet, args("full"));

    let returned = |value: &Value| {
        value["projection"]["page"]["returned"]
            .as_u64()
            .expect("returned expansion count")
    };
    assert!(returned(&compact) < returned(&balanced));
    assert!(returned(&balanced) < returned(&full));
    let compact_relations = relation_set(&compact);
    let balanced_relations = relation_set(&balanced);
    let full_relations = relation_set(&full);
    assert!(compact_relations.is_subset(&balanced_relations));
    assert!(compact_relations.len() < balanced_relations.len());
    assert!(balanced_relations.is_subset(&full_relations));
    assert!(balanced_relations.len() < full_relations.len());
    assert!(
        compact["projection"]["excluded_by_detail"].as_u64()
            > balanced["projection"]["excluded_by_detail"].as_u64()
    );
    assert!(
        balanced["projection"]["excluded_by_detail"].as_u64()
            > full["projection"]["excluded_by_detail"].as_u64()
    );
    assert_eq!(full["projection"]["excluded_by_detail"], 0);
}

#[test]
fn advisory_token_hint_does_not_filter_structured_content() {
    let packet = large_fixture(80);
    let args = |tokens: u32| {
        json!({
            "about": "project:kmp",
            "question": "What is current?",
            "budget": {"tokens": tokens, "max_bytes": 60_000, "detail": "full"}
        })
    };
    let tiny_hint = projected(packet.clone(), args(1));
    let large_hint = projected(packet, args(30_000));

    assert_eq!(tiny_hint["wake"], large_hint["wake"]);
    assert_eq!(tiny_hint["proof"], large_hint["proof"]);
    assert_eq!(
        tiny_hint["projection"]["page"]["returned"],
        large_hint["projection"]["page"]["returned"]
    );
    assert_eq!(tiny_hint["projection"]["budget"]["tokens_advisory"], 1);
    assert_eq!(
        large_hint["projection"]["budget"]["tokens_advisory"],
        30_000
    );
}

#[test]
fn byte_budget_sweep_is_monotone_deterministic_and_exactly_accounted() {
    let packet = large_fixture(120);
    let estimator = Cl100kEstimator::new();
    let mut previous_path = 0usize;
    for max_bytes in (3_000..=10_000).step_by(250) {
        let arguments = json!({
            "about": "project:kmp",
            "question": "Which storage engine is current?",
            "budget": {"tokens": 30_000, "max_bytes": max_bytes, "detail": "full"}
        });
        let outputs = (0..3)
            .map(|_| {
                project_recall_output(packet.clone(), &arguments, 2_400, &estimator)
                    .expect("projection")
                    .projected()
            })
            .collect::<Vec<_>>();
        let serialized = outputs
            .iter()
            .map(|value| serde_json::to_vec(value).expect("serialized projection"))
            .collect::<Vec<_>>();
        assert!(serialized.windows(2).all(|pair| pair[0] == pair[1]));
        assert!(serialized[0].len() <= max_bytes);
        assert_eq!(
            outputs[0]["projection"]["budget"]["used_bytes"],
            serialized[0].len()
        );
        let path = outputs[0]["proof"]["path"].as_array().expect("path").len();
        assert!(path >= previous_path, "larger byte budget lost proof path");
        previous_path = path;
    }
}
