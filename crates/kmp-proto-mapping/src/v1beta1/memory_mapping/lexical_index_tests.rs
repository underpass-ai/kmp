use super::answer_candidate_terms::AnswerCandidateTerms;
use super::answer_recall_context::AnswerRecallContext;
use super::bundle_views::answer_evidence_from_bundle;
use super::lexical_index_cache::LexicalIndexCache;
use super::lexical_index_identity::LexicalIndexIdentity;
use super::memory_lifecycle::MemoryLifecycle;
use super::{AskRetrievalContext, LexicalBridge, ask_response_from_result};
use kmp_application::{GetContextResult, MemoryAnswerPolicy, queries::render_graph_bundle};
use kmp_domain::{
    BundleMetadata, BundleNode, BundleRelationship, CaseId, GraphReadRevision, KmpBundle,
    RelationExplanation, RelationSemanticClass, Role, TemporalAxis, TemporalCursor,
    TemporalSelection,
};
use std::sync::Arc;

pub(super) fn fixture(count: usize, vocabulary: usize) -> GetContextResult {
    let node = |id: String, kind, text: String| {
        BundleNode::new(
            id.clone(),
            kind,
            id,
            text,
            "ACTIVE",
            vec![],
            Default::default(),
        )
    };
    let mut nodes: Vec<_> = (0..count)
        .map(|i| {
            let text = if i % 3 == 0 {
                "The cache valkey rollout was delayed during the audit."
            } else {
                "The invoice supplier payment was approved during the meeting."
            };
            node(
                format!("entry:{i}"),
                "observation",
                format!(
                    "{text} {}",
                    (0..vocabulary)
                        .map(|n| format!("vocabulary{n}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
            )
        })
        .collect();
    nodes.push(node(
        "dimension:task:proof".into(),
        "memory_dimension",
        "proof".into(),
    ));
    let relationships = (0..count)
        .map(|i| {
            BundleRelationship::new(
                "dimension:task:proof",
                format!("entry:{i}"),
                "contains_entry",
                RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("task")
                    .with_scope_id("proof")
                    .with_sequence((i + 1) as u32)
                    .with_observed_at(if i % 2 == 0 {
                        "2026-09-01T00:00:00Z"
                    } else {
                        "2026-09-02T00:00:00Z"
                    }),
            )
        })
        .collect();
    let bundle = KmpBundle::new(
        CaseId::new("project:lexical").expect("fixture"),
        Role::new("answerer").expect("fixture"),
        node(
            "project:lexical".into(),
            "memory_anchor",
            "Lexical control".into(),
        ),
        nodes,
        relationships,
        vec![],
        BundleMetadata::initial("test"),
    )
    .expect("fixture");
    GetContextResult {
        rendered: render_graph_bundle(&bundle),
        bundle,
        read_revision: Some(GraphReadRevision::new("store:1:revision:1").expect("fixture")),
        requested_scopes: vec!["task:proof".into()],
        served_at: std::time::SystemTime::UNIX_EPOCH,
        timing: None,
    }
}

pub(super) fn prepared(
    result: &GetContextResult,
) -> Vec<(kmp_proto::v1beta1::MemoryEvidence, AnswerCandidateTerms)> {
    let context = AnswerRecallContext::from_bundle_with_lifecycle(
        &result.bundle,
        MemoryLifecycle::read(&result.bundle),
    );
    answer_evidence_from_bundle(&result.bundle)
        .into_iter()
        .map(|item| {
            let terms = AnswerCandidateTerms::from_evidence(&item, &context);
            (item, terms)
        })
        .collect()
}

#[test]
fn reuses_statistics_but_rebuilds_for_revision_scope_clock_and_exact_terms() {
    let cache = LexicalIndexCache::default();
    let result = fixture(24, 4);
    let terms = prepared(&result);
    let identity =
        LexicalIndexIdentity::read(&result, &TemporalSelection::Frontier).expect("identity");
    assert!(!terms.is_empty());
    let first = cache.collection(Some(&identity), &terms);
    assert!(Arc::ptr_eq(
        &first,
        &cache.collection(Some(&identity), &terms)
    ));
    let mut revision = result.clone();
    revision.read_revision = Some(GraphReadRevision::new("store:1:revision:2").expect("revision"));
    let mut scope = result.clone();
    scope.requested_scopes = vec!["task:other".into()];
    let mut store = result.clone();
    store.read_revision = Some(GraphReadRevision::new("store:2:revision:1").expect("revision"));
    for changed in [&revision, &scope, &store] {
        let changed =
            LexicalIndexIdentity::read(changed, &TemporalSelection::Frontier).expect("identity");
        let original = cache.collection(Some(&identity), &terms);
        assert!(!Arc::ptr_eq(
            &original,
            &cache.collection(Some(&changed), &terms)
        ));
    }
    for axis in [
        TemporalAxis::Occurred,
        TemporalAxis::Observed,
        TemporalAxis::Ingested,
        TemporalAxis::Validity,
    ] {
        let selection = TemporalSelection::as_of(
            TemporalCursor::time("2026-09-01T00:00:00Z").expect("time"),
            axis,
        )
        .expect("selection");
        let key = LexicalIndexIdentity::read(&result, &selection).expect("identity");
        let original = cache.collection(Some(&identity), &terms);
        assert!(!Arc::ptr_eq(
            &original,
            &cache.collection(Some(&key), &terms)
        ));
    }
    // Even a caller with an incorrectly reused snapshot identity cannot reuse
    // changed lexical inputs (including count, order and morphology changes).
    let changed = prepared(&fixture(24, 5));
    let original = cache.collection(Some(&identity), &terms);
    assert!(!Arc::ptr_eq(
        &original,
        &cache.collection(Some(&identity), &changed)
    ));
    assert!(!Arc::ptr_eq(
        &cache.collection(Some(&identity), &terms),
        &cache.collection(Some(&identity), &terms[..12])
    ));
}

#[test]
fn unsupported_revisions_and_oversized_collections_are_read_without_retention() {
    let cache = LexicalIndexCache::default();
    let result = fixture(24, 0);
    let mut terms = prepared(&result);
    let key = LexicalIndexIdentity::read(&result, &TemporalSelection::Frontier).expect("identity");
    assert!(!Arc::ptr_eq(
        &cache.collection(None, &terms),
        &cache.collection(None, &terms)
    ));
    // One large key checks byte admission without allocating quadratic pairs.
    terms[0].1.direct_counts.insert("x".repeat(3 * 1024 * 1024));
    assert!(!Arc::ptr_eq(
        &cache.collection(Some(&key), &terms),
        &cache.collection(Some(&key), &terms)
    ));
}

#[test]
fn cached_complete_answers_match_fresh_ranking_across_policies_questions_and_mutations() {
    let cache = Arc::new(LexicalIndexCache::default());
    let bridge = LexicalBridge::none();
    let current = fixture(24, 4);
    let mut revised = fixture(25, 5);
    revised.read_revision = Some(GraphReadRevision::new("store:1:revision:2").expect("revision"));
    let historical = TemporalSelection::as_of(
        TemporalCursor::time("2026-09-01T12:00:00Z").expect("time"),
        TemporalAxis::Observed,
    )
    .expect("selection");
    for result in [&current, &revised, &current] {
        for selection in [&TemporalSelection::Frontier, &historical] {
            for policy in [
                MemoryAnswerPolicy::EvidenceOrUnknown,
                MemoryAnswerPolicy::BestEffort,
                MemoryAnswerPolicy::ShowConflicts,
            ] {
                for question in [
                    "cache valkey rollout",
                    "invoice supplier",
                    "zzzzunrelated zeppelin",
                    "",
                    "caché válido",
                ] {
                    let fresh = ask_response_from_result(
                        question,
                        None,
                        policy,
                        None,
                        result.clone(),
                        &bridge,
                        selection,
                    )
                    .expect("fresh");
                    if question == "zzzzunrelated zeppelin" {
                        assert_eq!(fresh.answer, "UNKNOWN");
                    }
                    if question == "cache valkey rollout" {
                        assert!(!fresh.because.is_empty());
                    }
                    for _ in 0..2 {
                        let cached = ask_response_from_result(
                            question,
                            None,
                            policy,
                            None,
                            AskRetrievalContext::from(result.clone())
                                .with_lexical_cache(Arc::clone(&cache)),
                            &bridge,
                            selection,
                        )
                        .expect("cached");
                        assert_eq!(fresh, cached, "{question} {policy:?} {selection:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn concurrent_publication_never_crosses_a_collection_and_readers_keep_their_index() {
    let cache = LexicalIndexCache::default();
    let first = fixture(24, 2);
    let second = fixture(30, 3);
    let first_terms = prepared(&first);
    let second_terms = prepared(&second);
    let key = LexicalIndexIdentity::read(&first, &TemporalSelection::Frontier).expect("identity");
    let pinned = cache.collection(Some(&key), &first_terms);
    std::thread::scope(|scope| {
        for n in 0..8 {
            let terms = if n % 2 == 0 {
                &first_terms
            } else {
                &second_terms
            };
            let cache = &cache;
            let key = &key;
            scope.spawn(move || {
                for _ in 0..8 {
                    assert!(cache.collection(Some(key), terms).matches(terms));
                }
            });
        }
    });
    assert!(pinned.matches(&first_terms));
    assert!(!pinned.matches(&second_terms));
}

#[test]
fn raw_bm25_scores_and_association_weights_match_fresh_statistics_exactly() {
    use super::association_index::AssociationIndex;
    use super::lexical_field::LexicalField;
    use super::term_counts::TermCounts;
    let cache = LexicalIndexCache::default();
    let result = fixture(24, 8);
    let key = LexicalIndexIdentity::read(&result, &TemporalSelection::Frontier).expect("identity");
    for result in [
        fixture(24, 8),
        fixture(25, 9),
        fixture(12, 8),
        fixture(24, 8),
    ] {
        let terms = prepared(&result);
        let fresh_content = LexicalField::build(terms.iter().map(|(_, t)| &t.content_counts));
        let fresh_direct = LexicalField::build(terms.iter().map(|(_, t)| &t.direct_counts));
        let fresh_associations =
            AssociationIndex::build(terms.iter().map(|(_, t)| &t.direct_counts));
        for _ in 0..2 {
            let cached = cache.collection(Some(&key), &terms);
            for word in [
                "cach",
                "valkey",
                "suppli",
                "invoic",
                "vocabulary0",
                "unrelated",
            ] {
                let question: TermCounts = [word.to_string()].into_iter().collect();
                let weights = fresh_associations.expand(&question);
                assert_eq!(cached.associations.expand(&question), weights);
                assert_eq!(
                    cached.direct.eligibility_floor(&question).to_bits(),
                    fresh_direct.eligibility_floor(&question).to_bits()
                );
                for (_, t) in &terms {
                    assert_eq!(
                        cached
                            .content
                            .score_weighted(&weights, &t.content_counts)
                            .to_bits(),
                        fresh_content
                            .score_weighted(&weights, &t.content_counts)
                            .to_bits()
                    );
                    assert_eq!(
                        cached
                            .direct
                            .score_weighted(&weights, &t.direct_counts)
                            .to_bits(),
                        fresh_direct
                            .score_weighted(&weights, &t.direct_counts)
                            .to_bits()
                    );
                }
            }
        }
    }
}

#[test]
fn lifecycle_changes_and_bridge_reconfiguration_do_not_reuse_query_decisions() {
    let cache = Arc::new(LexicalIndexCache::default());
    let original = fixture(24, 0);
    let mut changed = original.clone();
    let mut relations = changed.bundle.relationships().to_vec();
    relations[0] = BundleRelationship::new(
        "dimension:task:proof",
        "entry:0",
        "contains_entry",
        relations[0]
            .explanation()
            .clone()
            .with_valid_until("2026-09-01T12:00:00Z"),
    );
    relations.push(BundleRelationship::new(
        "entry:6",
        "entry:3",
        "supersedes",
        RelationExplanation::new(RelationSemanticClass::Evidential)
            .with_rationale("The revised audit supersedes the previous plan.")
            .with_evidence("The revised plan explicitly replaces entry 3."),
    ));
    changed.bundle = KmpBundle::new(
        changed.bundle.root_node_id().clone(),
        changed.bundle.role().clone(),
        changed.bundle.root_node().clone(),
        changed.bundle.neighbor_nodes().to_vec(),
        relations,
        changed.bundle.node_details().to_vec(),
        changed.bundle.metadata().clone(),
    )
    .expect("lifecycle fixture");
    changed.rendered = render_graph_bundle(&changed.bundle);
    changed.read_revision = Some(GraphReadRevision::new("store:1:revision:2").expect("revision"));
    let table = super::lexical_bridge::tests::table(
        "test-bridge",
        &[
            ("factura", &[127, 0]),
            ("invoice", &[127, 0]),
            ("cache", &[0, 127]),
        ],
    );
    let bridge = LexicalBridge::from_bytes(&table).expect("bridge");
    let before_expiry = TemporalSelection::as_of(
        TemporalCursor::time("2026-09-01T06:00:00Z").expect("time"),
        TemporalAxis::Observed,
    )
    .expect("selection");
    for result in [&original, &changed, &original] {
        for selection in [&TemporalSelection::Frontier, &before_expiry] {
            for bridge in [&LexicalBridge::none(), &bridge, &LexicalBridge::none()] {
                for question in ["cache valkey rollout", "factura", "invoice supplier"] {
                    let fresh = ask_response_from_result(
                        question,
                        None,
                        MemoryAnswerPolicy::EvidenceOrUnknown,
                        None,
                        result.clone(),
                        bridge,
                        selection,
                    )
                    .expect("fresh");
                    let cached = ask_response_from_result(
                        question,
                        None,
                        MemoryAnswerPolicy::EvidenceOrUnknown,
                        None,
                        AskRetrievalContext::from(result.clone())
                            .with_lexical_cache(Arc::clone(&cache)),
                        bridge,
                        selection,
                    )
                    .expect("cached");
                    assert_eq!(cached, fresh);
                    if std::ptr::eq(result, &changed) && selection.is_frontier() {
                        let proof = cached.proof.as_ref().expect("proof");
                        assert!(proof.expired.iter().any(|item| item.r#ref == "entry:0"));
                        assert!(proof.superseded.iter().any(|item| item.r#ref == "entry:3"));
                    }
                }
            }
        }
    }
}
