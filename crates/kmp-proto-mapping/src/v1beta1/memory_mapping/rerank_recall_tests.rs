use std::collections::BTreeMap;

use kmp_application::{GetContextResult, MemoryAnswerPolicy, queries::render_graph_bundle};
use kmp_domain::{
    BundleMetadata, BundleNode, BundleRelationship, CaseId, KmpBundle, RelationExplanation,
    RelationSemanticClass, Role, TemporalAxis, TemporalCursor, TemporalSelection,
};
use sha2::{Digest, Sha256};

use super::{AskRetrievalContext, LexicalBridge, RerankCandidateRanking, ask_response_from_result};

const QUESTION: &str = "Who fixed the car?";
const LEXICAL: &str = "The car was fixed by Ana on Monday.";
const PARAPHRASE: &str = "A technician repaired the automobile's brakes.";

fn context(paraphrase_expires: bool) -> GetContextResult {
    let node = |id: &str, kind: &str, text: &str| {
        BundleNode::new(id, kind, id, text, "ACTIVE", Vec::new(), BTreeMap::new())
    };
    let coordinate = |sequence: u32, expires: bool| {
        let explanation = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension("task")
            .with_scope_id("test")
            .with_sequence(sequence)
            .with_occurred_at("2026-01-02T00:00:00Z");
        if expires {
            explanation.with_valid_until("2026-01-03T00:00:00Z")
        } else {
            explanation
        }
    };
    let bundle = KmpBundle::new(
        CaseId::new("project:test").expect("fixture"),
        Role::new("answerer").expect("fixture"),
        node("project:test", "memory_anchor", "memory"),
        vec![
            node("dimension:test", "memory_dimension", "task"),
            node("entry:a", "observation", LEXICAL),
            node("entry:b", "observation", PARAPHRASE),
        ],
        vec![
            BundleRelationship::new(
                "dimension:test",
                "entry:a",
                "contains_entry",
                coordinate(1, false),
            ),
            BundleRelationship::new(
                "dimension:test",
                "entry:b",
                "contains_entry",
                coordinate(2, paraphrase_expires),
            ),
        ],
        Vec::new(),
        BundleMetadata::initial("test"),
    )
    .expect("fixture");
    let rendered = render_graph_bundle(&bundle);
    GetContextResult {
        read_revision: None,
        bundle,
        rendered,
        requested_scopes: Vec::new(),
        served_at: std::time::SystemTime::UNIX_EPOCH,
        timing: None,
    }
}

fn sha(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn ask(
    context: AskRetrievalContext,
    question: &str,
) -> Result<kmp_proto::v1beta1::AskResponse, String> {
    ask_response_from_result(
        question,
        None,
        MemoryAnswerPolicy::EvidenceOrUnknown,
        None,
        context,
        &LexicalBridge::none(),
        &TemporalSelection::Frontier,
    )
    .map_err(|status| status.message().to_string())
}

#[test]
fn the_pool_reads_the_ranker_first_then_what_it_left_out() {
    let pool = AskRetrievalContext::from(context(false))
        .rerank_pool(
            QUESTION,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            &TemporalSelection::Frontier,
            &LexicalBridge::none(),
            40,
        )
        .expect("pool");
    let refs = pool
        .iter()
        .map(|source| source.entry_ref.as_str())
        .collect::<Vec<_>>();
    assert_eq!(refs.first(), Some(&"entry:a"), "{refs:?}");
    assert!(
        refs.contains(&"entry:b"),
        "the paraphrase shares no word yet is read: {refs:?}"
    );
    assert_eq!(
        pool.iter()
            .find(|s| s.entry_ref == "entry:b")
            .expect("b")
            .text_sha256,
        sha(PARAPHRASE)
    );
    let one = AskRetrievalContext::from(context(false))
        .rerank_pool(
            QUESTION,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            &TemporalSelection::Frontier,
            &LexicalBridge::none(),
            1,
        )
        .expect("pool");
    assert_eq!(one.len(), 1);
}

#[test]
fn an_expired_entry_never_reaches_the_pool() {
    let after = TemporalSelection::as_of(
        TemporalCursor::time("2026-01-04T00:00:00Z").expect("fixture"),
        TemporalAxis::Occurred,
    )
    .expect("fixture");
    let pool = AskRetrievalContext::from(context(true))
        .rerank_pool(
            QUESTION,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            &after,
            &LexicalBridge::none(),
            40,
        )
        .expect("pool");
    assert!(
        pool.iter().all(|source| source.entry_ref != "entry:b"),
        "{pool:?}"
    );
}

#[test]
fn a_rerank_rescues_proof_without_touching_the_cited_answer() {
    let baseline = ask(context(false).into(), QUESTION).expect("baseline");
    let ranking = RerankCandidateRanking::new(
        "jev-1.13.0".into(),
        QUESTION,
        vec![
            ("entry:b".into(), sha(PARAPHRASE)),
            ("entry:a".into(), sha(LEXICAL)),
        ],
    )
    .expect("ranking");
    let reranked = ask(
        AskRetrievalContext::from(context(false)).with_rerank_candidates(ranking),
        QUESTION,
    )
    .expect("reranked");
    assert_eq!(reranked.answer, baseline.answer);
    assert_eq!(reranked.because, baseline.because);
    let proof = reranked.proof.expect("proof");
    assert_eq!(proof.confidence, baseline.proof.expect("proof").confidence);
    let rescued = proof
        .evidence
        .iter()
        .find(|item| item.text == PARAPHRASE)
        .expect("the paraphrase is in proof");
    assert_eq!(rescued.metadata["reached_by"], "rerank");
    assert_eq!(rescued.metadata["rerank_model"], "jev-1.13.0");
    let lexical = proof
        .evidence
        .iter()
        .find(|item| item.text == LEXICAL)
        .expect("lexical");
    assert!(
        !lexical.metadata.contains_key("rerank_model"),
        "a lexical item keeps its provenance"
    );
    assert!(
        reranked
            .warnings
            .iter()
            .any(|w| w.contains("evidence rerank by jev-1.13.0"))
    );
}

#[test]
fn a_ranking_for_another_question_is_refused() {
    let ranking =
        RerankCandidateRanking::new("m".into(), "another question", vec![]).expect("ranking");
    let error = ask(
        AskRetrievalContext::from(context(false)).with_rerank_candidates(ranking),
        QUESTION,
    )
    .expect_err("refused");
    assert!(error.contains("different question"), "{error}");
}
