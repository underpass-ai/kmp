//! A wake claim cites evidence by identity, never by copying its prose into
//! the field that the projection resolves against `proof.evidence[].id`.
use super::responses::wake_response_from_result;
use kmp_application::{GetContextResult, queries::render_graph_bundle};
use kmp_domain::{
    BundleMetadata, BundleNode, BundleNodeDetail, BundleRelationship, CaseId, KmpBundle,
    RelationExplanation, RelationSemanticClass, Role, TemporalSelection,
};
use kmp_proto::v1beta1::WakeResponse;
use std::collections::{BTreeMap, BTreeSet};

const MEASURED: &str = "Load test measured 40 ms at p95.";

fn node(id: &str, kind: &str) -> BundleNode {
    BundleNode::new(id, kind, id, id, "ACTIVE", Vec::new(), BTreeMap::new())
}

fn edge(source: &str, target: &str, rel: &str, class: RelationSemanticClass) -> BundleRelationship {
    BundleRelationship::new(source, target, rel, RelationExplanation::new(class))
}

fn explained(
    source: &str,
    target: &str,
    rel: &str,
    why: &str,
    evidence: &str,
) -> BundleRelationship {
    BundleRelationship::new(
        source,
        target,
        rel,
        RelationExplanation::new(RelationSemanticClass::Causal)
            .with_rationale(why)
            .with_evidence(evidence),
    )
}

/// Two sources carry the same measurement text; only one of them supports
/// the decision the relation starts from. A third relation cites prose that
/// no stored source holds.
fn wake() -> WakeResponse {
    wake_with_padding(0)
}

/// `padding` unrelated sources, each larger than the cited one, compete for
/// the first page.
fn wake_with_padding(padding: usize) -> WakeResponse {
    let structural = RelationSemanticClass::Structural;
    let evidential = RelationSemanticClass::Evidential;
    let pad_ids = (0..padding)
        .map(|index| format!("evidence:pad-{index:02}"))
        .collect::<Vec<_>>();
    let mut nodes = vec![
        node("lane", "lane"),
        node("decision:cache", "memory_entry"),
        node("task:rollout", "memory_entry"),
        node("note:other", "memory_entry"),
        node("evidence:load-test", "memory_evidence"),
        node("evidence:old-bench", "memory_evidence"),
    ];
    nodes.extend(pad_ids.iter().map(|id| node(id, "memory_evidence")));
    let mut edges = vec![
        edge("lane", "decision:cache", "contains_entry", structural),
        edge("lane", "task:rollout", "contains_entry", structural),
        edge("lane", "note:other", "contains_entry", structural),
        edge(
            "evidence:load-test",
            "decision:cache",
            "supports",
            evidential,
        ),
        edge("evidence:old-bench", "note:other", "supports", evidential),
        explained(
            "decision:cache",
            "task:rollout",
            "updates_state",
            "The cache decision unblocks the rollout.",
            MEASURED,
        ),
        explained(
            "note:other",
            "task:rollout",
            "depends_on",
            "The rollout waits on the note.",
            "Someone said so in a meeting.",
        ),
    ];
    edges.extend(
        pad_ids
            .iter()
            .map(|id| edge(id, "note:other", "supports", evidential)),
    );
    let mut details = vec![
        BundleNodeDetail::new("evidence:load-test", MEASURED, "h1", 1),
        BundleNodeDetail::new("evidence:old-bench", MEASURED, "h2", 1),
    ];
    details.extend(pad_ids.iter().map(|id| {
        BundleNodeDetail::new(
            id,
            format!("{id}: {}", "unrelated observation ".repeat(20)),
            id,
            1,
        )
    }));
    let bundle = KmpBundle::new(
        CaseId::new("about:x").expect("about"),
        Role::new("reader").expect("role"),
        node("about:x", "about"),
        nodes,
        edges,
        details,
        BundleMetadata::initial("test"),
    )
    .expect("bundle");
    let rendered = render_graph_bundle(&bundle);
    let result = GetContextResult {
        read_revision: None,
        bundle,
        rendered,
        requested_scopes: Vec::new(),
        served_at: std::time::SystemTime::UNIX_EPOCH,
        timing: None,
    };
    wake_response_from_result("resume", None, result, &TemporalSelection::Frontier).expect("wake")
}

fn claim<'a>(response: &'a WakeResponse, claim: &str) -> &'a kmp_proto::v1beta1::WakeClaim {
    response
        .wake
        .as_ref()
        .expect("wake packet")
        .causal_spine
        .iter()
        .find(|candidate| candidate.claim == claim)
        .unwrap_or_else(|| panic!("claim {claim} in causal spine"))
}

#[test]
fn a_claim_cites_the_source_that_supports_its_relation_by_id() {
    let response = wake();
    let cited = claim(&response, "decision:cache -> task:rollout");

    assert_eq!(cited.evidence_refs, ["detail:evidence:load-test"]);
    assert!(
        cited.evidence.is_empty(),
        "the body lives once in proof.evidence"
    );
}

#[test]
fn equal_text_from_an_unrelated_source_is_not_the_same_evidence() {
    let response = wake();
    let cited = claim(&response, "decision:cache -> task:rollout");

    assert!(
        !cited
            .evidence_refs
            .contains(&"detail:evidence:old-bench".to_string()),
        "same body, different provenance"
    );
}

#[test]
fn prose_no_source_holds_stays_inline_and_is_never_a_ref() {
    let response = wake();
    let cited = claim(&response, "note:other -> task:rollout");

    assert!(cited.evidence_refs.is_empty());
    assert_eq!(cited.evidence, "Someone said so in a meeting.");
}

#[test]
fn every_emitted_ref_resolves_in_the_proof_or_its_gaps() {
    let response = wake();
    let proof = response.proof.as_ref().expect("proof");
    let known = proof
        .evidence
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();

    for claim in &response.wake.as_ref().expect("wake packet").causal_spine {
        for evidence_ref in &claim.evidence_refs {
            assert!(
                known.contains(evidence_ref.as_str()),
                "{} cites unresolved {evidence_ref}",
                claim.claim
            );
        }
    }
}

#[test]
fn the_cited_source_is_pinned_in_the_first_page_under_a_small_budget() {
    use crate::v1beta1::recall_projection::project_wake_response;
    use kmp_proto::v1beta1::{MemoryBudget, WakeRequest};

    let request = WakeRequest {
        about: "about:x".to_string(),
        budget: Some(MemoryBudget {
            max_bytes: 4_000,
            ..Default::default()
        }),
        ..Default::default()
    };
    let page = project_wake_response(wake_with_padding(24), &request).expect("first page");
    let evidence = page
        .proof
        .as_ref()
        .expect("proof")
        .evidence
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();

    assert!(
        evidence.contains("detail:evidence:load-test"),
        "the source a claim cites travels with the claim: {evidence:?}"
    );
}
