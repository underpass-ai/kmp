use std::collections::BTreeMap;

use kmp_application::{GetContextResult, MemoryAnswerPolicy, queries::render_graph_bundle};
use kmp_domain::{
    BundleMetadata, BundleNode, BundleRelationship, CaseId, KmpBundle, RelationExplanation,
    RelationSemanticClass, Role, TemporalAxis, TemporalCursor, TemporalSelection,
};
use sha2::{Digest, Sha256};

use super::{
    AskRetrievalContext, LexicalBridge, SemanticCandidateRanking, ask_response_from_result,
};

const TEXT: &str = "The automobile was repaired by a technician.";

fn context(expired: bool) -> GetContextResult {
    let node = |id: &str, kind: &str, text: &str| {
        BundleNode::new(id, kind, id, text, "ACTIVE", Vec::new(), BTreeMap::new())
    };
    let mut coordinate = RelationExplanation::new(RelationSemanticClass::Structural)
        .with_dimension("task")
        .with_scope_id("test")
        .with_sequence(1)
        .with_occurred_at("2026-01-02T00:00:00Z");
    if expired {
        coordinate = coordinate.with_valid_until("2026-01-03T00:00:00Z");
    }
    let bundle = KmpBundle::new(
        CaseId::new("project:test").expect("valid test fixture"),
        Role::new("answerer").expect("valid test fixture"),
        node("project:test", "memory_anchor", "memory"),
        vec![
            node("dimension:test", "memory_dimension", "task"),
            node("entry:a", "observation", TEXT),
        ],
        vec![BundleRelationship::new(
            "dimension:test",
            "entry:a",
            "contains_entry",
            coordinate,
        )],
        Vec::new(),
        BundleMetadata::initial("test"),
    )
    .expect("valid test fixture");
    let rendered = render_graph_bundle(&bundle);
    GetContextResult {
        bundle,
        rendered,
        requested_scopes: Vec::new(),
        served_at: std::time::SystemTime::UNIX_EPOCH,
        timing: None,
    }
}

fn ranking(entry_ref: &str, text: &str) -> SemanticCandidateRanking {
    SemanticCandidateRanking::new(
        "encoder@immutable-revision".into(),
        "mechanic fixed car",
        vec![(
            entry_ref.into(),
            format!("{:x}", Sha256::digest(text.as_bytes())),
        )],
    )
    .expect("valid test fixture")
}

fn ask(
    context: AskRetrievalContext,
    temporal: &TemporalSelection,
    limit: Option<usize>,
) -> kmp_proto::v1beta1::AskResponse {
    ask_response_from_result(
        "mechanic fixed car",
        None,
        MemoryAnswerPolicy::EvidenceOrUnknown,
        limit,
        context,
        &LexicalBridge::none(),
        temporal,
    )
    .expect("valid test fixture")
}

#[test]
fn paraphrase_can_retrieve_a_source_while_the_answer_remains_unknown() {
    let baseline = ask(context(false).into(), &TemporalSelection::Frontier, None);
    let hybrid = ask(
        AskRetrievalContext::from(context(false))
            .with_semantic_candidates(ranking("entry:a", TEXT)),
        &TemporalSelection::Frontier,
        None,
    );
    assert_eq!(baseline.answer, "UNKNOWN");
    assert_eq!(hybrid.answer, baseline.answer);
    assert_eq!(hybrid.because, baseline.because);
    let proof = hybrid.proof.expect("valid test fixture");
    assert_eq!(
        proof.confidence,
        baseline.proof.expect("valid test fixture").confidence
    );
    let found = proof
        .evidence
        .iter()
        .find(|item| item.text == TEXT)
        .expect("semantic evidence");
    assert_eq!(found.metadata["reached_by"], "semantic");
    assert_eq!(found.source, "entry:a");
}

#[test]
fn semantic_candidates_cannot_escape_scope_time_lifecycle_or_content_version() {
    let before = TemporalSelection::as_of(
        TemporalCursor::time("2026-01-01T00:00:00Z").expect("valid test fixture"),
        TemporalAxis::Occurred,
    )
    .expect("valid test fixture");
    let after_expiry = TemporalSelection::as_of(
        TemporalCursor::time("2026-01-04T00:00:00Z").expect("valid test fixture"),
        TemporalAxis::Occurred,
    )
    .expect("valid test fixture");
    for (ctx, temporal, proposed) in [
        (
            context(false),
            TemporalSelection::Frontier,
            ranking("foreign:entry:a", TEXT),
        ),
        (
            context(false),
            TemporalSelection::Frontier,
            ranking("entry:a", "The automobile was not repaired."),
        ),
        (context(false), before, ranking("entry:a", TEXT)),
        (context(true), after_expiry, ranking("entry:a", TEXT)),
    ] {
        let response = ask(
            AskRetrievalContext::from(ctx).with_semantic_candidates(proposed),
            &temporal,
            None,
        );
        assert!(
            response.proof.expect("valid test fixture").evidence.iter().all(|item| item
                .metadata
                .get("reached_by")
                .map(String::as_str)
                != Some("semantic"))
        );
        assert!(response.because.is_empty());
    }
}

#[test]
fn explicit_entry_cap_applies_to_semantic_proof_as_well() {
    let response = ask(
        AskRetrievalContext::from(context(false))
            .with_semantic_candidates(ranking("entry:a", TEXT)),
        &TemporalSelection::Frontier,
        Some(0),
    );
    assert!(
        response
            .proof
            .expect("valid test fixture")
            .evidence
            .is_empty()
    );
}

#[test]
fn a_ranking_cannot_be_replayed_for_a_different_question() {
    let result = ask_response_from_result(
        "unrelated question",
        None,
        MemoryAnswerPolicy::EvidenceOrUnknown,
        None,
        AskRetrievalContext::from(context(false))
            .with_semantic_candidates(ranking("entry:a", TEXT)),
        &LexicalBridge::none(),
        &TemporalSelection::Frontier,
    );
    assert!(result.is_err());
}

#[test]
fn separate_lexical_channel_obeys_admission_and_never_establishes_an_answer() {
    let cases = [
        (
            context(false),
            TemporalSelection::Frontier,
            "entry:a",
            TEXT,
            true,
        ),
        (
            context(false),
            TemporalSelection::Frontier,
            "foreign:entry:a",
            TEXT,
            false,
        ),
        (
            context(false),
            TemporalSelection::Frontier,
            "entry:a",
            "changed text",
            false,
        ),
        (
            context(false),
            TemporalSelection::as_of(
                TemporalCursor::time("2026-01-01T00:00:00Z").expect("time"),
                TemporalAxis::Occurred,
            )
            .expect("selection"),
            "entry:a",
            TEXT,
            false,
        ),
        (
            context(true),
            TemporalSelection::as_of(
                TemporalCursor::time("2026-01-04T00:00:00Z").expect("time"),
                TemporalAxis::Occurred,
            )
            .expect("selection"),
            "entry:a",
            TEXT,
            false,
        ),
    ];
    for (ctx, temporal, entry_ref, text, expected) in cases {
        let ranking = SemanticCandidateRanking::new(
            "encoder@immutable-revision/separate-bm25-v1".into(),
            "mechanic fixed car",
            Vec::new(),
        )
        .expect("dense channel")
        .with_lexical_candidates(vec![(
            entry_ref.into(),
            format!("{:x}", Sha256::digest(text.as_bytes())),
        )])
        .expect("lexical channel");
        let response = ask(
            AskRetrievalContext::from(ctx).with_semantic_candidates(ranking),
            &temporal,
            None,
        );
        assert_eq!(response.answer, "UNKNOWN");
        assert!(response.because.is_empty());
        let proof = response.proof.expect("proof");
        assert_eq!(
            proof.evidence.iter().any(|item| item
                .metadata
                .get("retrieval_channel")
                .map(String::as_str)
                == Some("bm25")),
            expected
        );
    }
}
