//! What the plan keeps and what it yields first: cited proof over capped
//! selections, and the catalogue head before the rest of the catalogue.

use std::collections::BTreeSet;

use serde_json::json;

use super::test_support::{
    fixture, labels_fixture, projected, typed_wake_fixture, wake_request_with_bytes,
};
use super::typed_recall::project_wake_response;

#[test]
fn cited_evidence_survives_max_entries_even_when_it_is_not_first() {
    let mut packet = fixture();
    packet["proof"]["evidence"]
        .as_array_mut()
        .expect("evidence")
        .reverse();
    let output = projected(
        packet,
        json!({
            "about": "project:kmp",
            "question": "What is current?",
            "budget": {
                "tokens": 10_000,
                "max_bytes": 20_000,
                "detail": "full",
                "max_entries": 1
            }
        }),
    );

    let evidence = output["proof"]["evidence"].as_array().expect("evidence");
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0]["id"], "evidence:a");
    assert_eq!(output["projection"]["selection_omitted"], 1);
}

#[test]
fn under_a_tight_budget_the_labels_yield_before_the_cited_proof() {
    let mut response = typed_wake_fixture(4);
    response.labels = labels_fixture(60);
    let cited = response
        .wake
        .as_ref()
        .map(|wake| {
            wake.causal_spine
                .iter()
                .map(|claim| claim.evidence_ref.clone())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();

    let generous = project_wake_response(response.clone(), &wake_request_with_bytes(60_000))
        .expect("generous projection");
    assert_eq!(
        generous.labels.len(),
        60,
        "a generous budget keeps the whole catalogue"
    );

    let tight =
        project_wake_response(response, &wake_request_with_bytes(4_000)).expect("tight projection");
    assert!(
        !tight.labels.is_empty() && tight.labels.len() < 60,
        "a tight budget keeps the head of the catalogue and drops the tail, kept {}",
        tight.labels.len()
    );
    assert_eq!(
        tight.labels[0].value, "underpass-ai-kmp-000",
        "the catalogue keeps its own order under the budget"
    );
    let retained = tight
        .proof
        .as_ref()
        .map(|proof| {
            proof
                .evidence
                .iter()
                .map(|item| item.id.clone())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    assert!(
        cited.is_subset(&retained),
        "the cited proof survives the budget the catalogue yielded to"
    );
    assert_eq!(
        tight
            .projection
            .as_ref()
            .map(|projection| projection.core_text_shortened),
        Some(false),
        "no word of the core is shortened to make room for a label"
    );
    assert_eq!(
        tight
            .truncation
            .as_ref()
            .map(|truncation| truncation.truncated),
        Some(true),
        "dropping labels is reported as truncation"
    );
}
