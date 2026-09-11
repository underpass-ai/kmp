use super::support::*;
use kmp_domain::*;

#[tokio::test]
async fn deep_paths_need_no_preknown_destination_and_every_work_cut_stays_partial() {
    let mut mutations = vec![node("s")];
    let names: Vec<_> = std::iter::once("s".to_owned())
        .chain((1..=100).map(|i| format!("n{i}")))
        .collect();
    for pair in names.windows(2) {
        mutations.extend([
            node(&pair[1]),
            edge(&pair[0], &pair[1], "uses_background", Some(EARLY)),
        ]);
    }
    let (_dir, store) = store(mutations).await;
    let q = request(vec![role("source", &vec!["uses_background"; 100], vec![])]);
    let full = store
        .load_evidence_paths(&q)
        .await
        .expect("temporal traversal control succeeds");
    assert_eq!(full.status, EvidencePathStatus::Compatible);
    assert_eq!(full.candidates[0].nodes, names);
    assert_eq!(full.candidates[0].edge_indexes.len(), 100);
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
        let mut limited = q.clone();
        limited.limits = limits;
        let cut = store
            .load_evidence_paths(&limited)
            .await
            .expect("temporal traversal control succeeds");
        assert_eq!(cut.status, EvidencePathStatus::Partial);
        assert!(!cut.known_complete());
        assert!(cut.work_states <= limits.states);
    }
}

#[tokio::test]
async fn a_future_identity_link_is_not_admitted_early_and_unknown_clock_needs_review() {
    let (_dir, store) = store(vec![
        node("s"),
        node("p"),
        node("q"),
        label("s", "task", "case", EARLY),
        label("p", "task", "case", EARLY),
        label("q", "task", "case", EARLY),
        edge("s", "p", "same_entity_as", Some(LATE)),
        edge("s", "q", "same_entity_as", None),
    ])
    .await;
    let mut q = request(vec![role(
        "identity",
        &["same_entity_as"],
        vec![EvidencePathBinding::Reference {
            at: 1,
            name: "person".into(),
        }],
    )]);
    q.temporal =
        TemporalSelection::as_of(TemporalCursor::Time(EARLY.into()), TemporalAxis::Observed)
            .expect("temporal traversal control succeeds");
    let early = store
        .load_evidence_paths(&q)
        .await
        .expect("temporal traversal control succeeds");
    assert_eq!(early.candidates.len(), 1);
    assert_eq!(early.candidates[0].nodes, ["s", "q"]);
    assert_eq!(early.status, EvidencePathStatus::ReviewRequired);
    assert!(!early.known_complete());
    q.temporal =
        TemporalSelection::as_of(TemporalCursor::Time(LATE.into()), TemporalAxis::Observed)
            .expect("temporal traversal control succeeds");
    let later = store
        .load_evidence_paths(&q)
        .await
        .expect("temporal traversal control succeeds");
    assert_eq!(later.candidates.len(), 2);
    assert!(later.known_complete());
    assert_eq!(later.status, EvidencePathStatus::ReviewRequired);
}

#[tokio::test]
async fn incoming_identity_keeps_its_stored_arrow_and_foreign_about_is_excluded() {
    let mut foreign = node("outside");
    if let ProjectionMutation::UpsertNode(n) = &mut foreign {
        n.properties
            .insert("memory_about".into(), "project:other".into());
    }
    let (_dir, store) = store(vec![
        node("s"),
        node("p"),
        foreign,
        edge("p", "s", "same_entity_as", Some(EARLY)),
        edge("outside", "s", "same_entity_as", Some(EARLY)),
    ])
    .await;
    let mut role = role("identity", &["same_entity_as"], vec![]);
    role.steps[0].direction = RelationDirection::Incoming;
    let got = store
        .load_evidence_paths(&request(vec![role]))
        .await
        .expect("temporal traversal control succeeds");
    assert_eq!(got.candidates.len(), 1);
    assert_eq!(got.candidates[0].nodes, ["s", "p"]);
    assert_eq!(got.relations[0].source_node_id, "p");
    assert_eq!(got.relations[0].target_node_id, "s");
    assert_eq!(
        got.relations[0].explanation.evidence(),
        Some("Record: p same_entity_as s.")
    );
}
