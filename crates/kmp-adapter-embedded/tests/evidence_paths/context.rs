use super::support::*;
use kmp_domain::*;

fn discovery(
    name: &str,
    relations: &[&str],
    bindings: Vec<EvidencePathBinding>,
) -> EvidencePathRole {
    let mut r = role(name, relations, bindings);
    r.context = true;
    r
}

#[tokio::test]
async fn context_finds_unannounced_mixed_sequences_preserves_ties_and_stored_arrows() {
    let (_dir, store) = store(vec![
        node("s"),
        node("a"),
        node("b"),
        node("x"),
        node("v"),
        edge("s", "a", "uses_background", Some(EARLY)),
        edge("b", "s", "same_entity_as", Some(EARLY)),
        edge("a", "x", "depends_on", Some(EARLY)),
        edge("x", "b", "follows", Some(EARLY)),
        edge("x", "v", "verified_by", Some(EARLY)),
    ])
    .await;
    let q = request(vec![discovery("verification", &["verified_by"], vec![])]);
    let found = store.load_evidence_paths(&q).await.expect("discovered");
    assert_eq!(found.status, EvidencePathStatus::ReviewRequired);
    assert!(!found.known_complete());
    assert_eq!(found.candidates.len(), 2);
    let paths: std::collections::BTreeSet<_> = found
        .candidates
        .iter()
        .map(|c| {
            assert_eq!(c.context_hops, 2);
            c.nodes.iter().map(String::as_str).collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        paths,
        [vec!["s", "a", "x", "v"], vec!["s", "b", "x", "v"]].into()
    );
    assert!(found.shared_states > 0);
    assert!(found.relations.iter().any(|e| e.source_node_id == "b"
        && e.target_node_id == "s"
        && e.relation_type == "same_entity_as"));
}

#[tokio::test]
async fn context_cycles_and_longer_prefixes_do_not_multiply_the_same_anchor() {
    let (_dir, store) = store(vec![
        node("s"),
        node("a"),
        node("x"),
        node("v"),
        edge("s", "a", "uses_background", Some(EARLY)),
        edge("a", "x", "uses_background", Some(EARLY)),
        edge("s", "x", "uses_background", Some(EARLY)),
        edge("x", "s", "follows", Some(EARLY)),
        edge("x", "v", "verified_by", Some(EARLY)),
    ])
    .await;
    let found = store
        .load_evidence_paths(&request(vec![discovery("v", &["verified_by"], vec![])]))
        .await
        .expect("discovered");
    // Two distinct one-hop declarations, with opposite stored arrows, survive.
    assert_eq!(found.candidates.len(), 2);
    assert!(
        found
            .candidates
            .iter()
            .all(|c| c.nodes == ["s", "x", "v"] && c.context_hops == 1)
    );
    assert_eq!(found.stop, TraceSearchStop::FrontierExhausted);
    assert!(found.work_states < 100);
}

#[tokio::test]
async fn context_binds_the_main_witness_after_discovery_and_keeps_explicit_suffix() {
    let (_dir, store) = store(vec![
        node("s"),
        node("x"),
        node("v"),
        node("p"),
        node("doc"),
        edge("s", "x", "follows", Some(EARLY)),
        edge("x", "v", "verified_by", Some(EARLY)),
        edge("p", "x", "authorizes", Some(EARLY)),
        edge("v", "doc", "uses_background", Some(EARLY)),
        label("v", "event", "A", EARLY),
        label("p", "event", "B", EARLY),
    ])
    .await;
    let verification = discovery(
        "verification",
        &["verified_by", "uses_background"],
        vec![binding(1)],
    );
    let mut permission = discovery("permission", &["authorizes"], vec![binding(1)]);
    permission.steps[0].direction = RelationDirection::Incoming;
    let found = store
        .load_evidence_paths(&request(vec![verification, permission]))
        .await
        .expect("discovered");
    assert_eq!(found.status, EvidencePathStatus::IncompatibleObligations);
    assert!(found.groups.is_empty());
    assert!(
        found
            .candidates
            .iter()
            .any(|c| c.nodes == ["s", "x", "v", "doc"]
                && c.bindings.domains["event"] == ["A".into()].into())
    );
    assert!(
        found
            .candidates
            .iter()
            .any(|c| c.nodes == ["s", "x", "p"]
                && c.bindings.domains["event"] == ["B".into()].into())
    );
}

#[tokio::test]
async fn context_admission_respects_clocks_about_proof_and_relation_direction() {
    let mut foreign = node("foreign");
    if let ProjectionMutation::UpsertNode(n) = &mut foreign {
        n.properties
            .insert("memory_about".into(), "project:elsewhere".into());
    }
    let mut no_proof = edge("s", "bad", "follows", Some(EARLY));
    if let ProjectionMutation::UpsertNodeRelation(e) = &mut no_proof {
        e.explanation = RelationExplanation::new(RelationSemanticClass::Procedural);
    }
    let mut mutations = vec![
        node("s"),
        node("future"),
        node("unknown"),
        node("bad"),
        foreign,
        node("v"),
        node("foreign-v"),
        node("future-v"),
        node("bad-v"),
        edge("s", "future", "follows", Some(LATE)),
        edge("future", "future-v", "verified_by", Some(EARLY)),
        edge("s", "unknown", "uses_background", None),
        edge("unknown", "v", "verified_by", Some(EARLY)),
        edge("s", "foreign", "follows", Some(EARLY)),
        edge("foreign", "foreign-v", "verified_by", Some(EARLY)),
        no_proof,
        edge("bad", "bad-v", "verified_by", Some(EARLY)),
    ];
    for n in [
        "s",
        "future",
        "unknown",
        "bad",
        "v",
        "foreign-v",
        "future-v",
        "bad-v",
    ] {
        mutations.push(label(n, "task", "test", EARLY));
    }
    let (_dir, store) = store(mutations).await;
    let mut q = request(vec![discovery("v", &["verified_by"], vec![])]);
    q.temporal =
        TemporalSelection::as_of(TemporalCursor::Time(EARLY.into()), TemporalAxis::Observed)
            .expect("cut");
    let found = store.load_evidence_paths(&q).await.expect("discovered");
    assert_eq!(found.candidates.len(), 1);
    assert_eq!(found.candidates[0].nodes, ["s", "unknown", "v"]);
    assert!(found.candidates[0].clock_unknown);
    assert!(!found.known_complete());
    q.roles[0].steps[0].direction = RelationDirection::Incoming;
    let inverse = store.load_evidence_paths(&q).await.expect("inverse");
    // Exploring the incoming declaration reaches its real source, never flips it.
    assert_eq!(
        inverse.candidates[0].nodes.last().map(String::as_str),
        Some("unknown")
    );
}

#[tokio::test]
async fn context_discovers_a_hundred_hops_without_a_sequence_and_cuts_stay_partial() {
    let mut mutations = vec![node("s"), node("v")];
    let names: Vec<_> = std::iter::once("s".to_owned())
        .chain((1..=100).map(|i| format!("n{i}")))
        .collect();
    for pair in names.windows(2) {
        mutations.extend([
            node(&pair[1]),
            edge(&pair[0], &pair[1], "uses_background", Some(EARLY)),
        ]);
    }
    mutations.push(edge("n100", "v", "verified_by", Some(EARLY)));
    let (_dir, store) = store(mutations).await;
    let q = request(vec![discovery("v", &["verified_by"], vec![])]);
    let full = store.load_evidence_paths(&q).await.expect("deep");
    assert_eq!(full.candidates.len(), 1);
    assert_eq!(full.candidates[0].context_hops, 100);
    assert_eq!(full.candidates[0].nodes.len(), 102);
    assert_eq!(full.stop, TraceSearchStop::FrontierExhausted);
    for limits in [
        TraceSearchLimits {
            nodes: 4,
            ..q.limits
        },
        TraceSearchLimits {
            edges: 4,
            ..q.limits
        },
        TraceSearchLimits {
            states: 4,
            ..q.limits
        },
        TraceSearchLimits {
            depth: 4,
            ..q.limits
        },
    ] {
        let mut cut = q.clone();
        cut.limits = limits;
        let got = store.load_evidence_paths(&cut).await.expect("cut");
        assert_eq!(got.status, EvidencePathStatus::Partial);
        assert!(!got.known_complete());
    }
}
