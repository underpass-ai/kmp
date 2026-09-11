use super::support::*;
use kmp_domain::*;

#[tokio::test]
async fn pairwise_compatible_event_witnesses_can_have_no_joint_proof() {
    let mut mutations = vec![node("s")];
    for (n, values) in [("a", ["A", "B"]), ("b", ["B", "C"]), ("c", ["A", "C"])] {
        mutations.extend([node(n), edge("s", n, n, Some(EARLY))]);
        mutations.extend(values.into_iter().map(|v| label(n, "event", v, EARLY)));
    }
    let (_dir, store) = store(mutations).await;
    let q = request(
        ["a", "b", "c"]
            .into_iter()
            .map(|r| role(r, &[r], vec![binding(1)]))
            .collect(),
    );
    let got = store
        .load_evidence_paths(&q)
        .await
        .expect("joint control succeeds");
    assert_eq!(got.candidates.len(), 3);
    assert!(got.missing_roles.is_empty());
    assert_eq!(got.status, EvidencePathStatus::IncompatibleObligations);
    assert!(!got.known_complete());
    assert!(got.groups.is_empty());
}

#[tokio::test]
async fn a_missing_witness_is_not_filled_from_another_route() {
    let (_dir, store) = store(vec![
        node("s"),
        node("a"),
        node("b"),
        edge("s", "a", "uses_background", Some(EARLY)),
        edge("s", "b", "verified_by", Some(EARLY)),
        label("a", "event", "A", EARLY),
    ])
    .await;
    let q = request(vec![
        role("execution", &["uses_background"], vec![binding(1)]),
        role("check", &["verified_by"], vec![binding(1)]),
    ]);
    let got = store
        .load_evidence_paths(&q)
        .await
        .expect("joint control succeeds");
    assert_eq!(got.status, EvidencePathStatus::ReviewRequired);
    assert!(!got.known_complete());
    assert_eq!(got.groups.len(), 1);
    assert_eq!(got.groups[0].bindings.domains["event"], ["A".into()].into());
    assert_eq!(
        got.groups[0]
            .bindings
            .missing
            .iter()
            .next()
            .expect("joint control succeeds")
            .reference,
        "b"
    );
}

#[tokio::test]
async fn same_node_with_different_obligations_retains_different_states() {
    let (_dir, store) = store(vec![
        node("s"),
        node("a"),
        node("b"),
        node("t"),
        node("v"),
        edge("s", "a", "uses_background", Some(EARLY)),
        edge("s", "b", "uses_background", Some(EARLY)),
        edge("a", "t", "uses_background", Some(EARLY)),
        edge("b", "t", "uses_background", Some(EARLY)),
        edge("s", "v", "verified_by", Some(EARLY)),
        label("a", "event", "A", EARLY),
        label("b", "event", "B", EARLY),
        label("v", "event", "A", EARLY),
    ])
    .await;
    let q = request(vec![
        role(
            "execution",
            &["uses_background", "uses_background"],
            vec![binding(1)],
        ),
        role("verification", &["verified_by"], vec![binding(1)]),
    ]);
    let got = store
        .load_evidence_paths(&q)
        .await
        .expect("joint control succeeds");
    assert_eq!(got.candidates.len(), 3);
    assert_eq!(got.groups.len(), 1);
    assert_eq!(got.status, EvidencePathStatus::Compatible);
    let route = &got.candidates[got.groups[0].candidate_indexes[0] as usize];
    assert_eq!(route.nodes, ["s", "a", "t"]);
    assert_eq!(got.shared_states, 0);
}

#[tokio::test]
async fn duplicate_proofs_do_not_create_identity_ambiguity() {
    let (_dir, store) = store(vec![
        node("s"),
        node("a"),
        node("b"),
        node("t"),
        node("u"),
        edge("s", "a", "uses_background", Some(EARLY)),
        edge("s", "b", "uses_background", Some(EARLY)),
        edge("a", "t", "uses_background", Some(EARLY)),
        edge("b", "t", "uses_background", Some(EARLY)),
        edge("t", "u", "same_entity_as", Some(EARLY)),
    ])
    .await;
    let q = request(vec![role(
        "identity",
        &["uses_background", "uses_background", "same_entity_as"],
        vec![EvidencePathBinding::Reference {
            at: 3,
            name: "person".into(),
        }],
    )]);
    let got = store
        .load_evidence_paths(&q)
        .await
        .expect("joint control succeeds");
    assert_eq!(got.candidates.len(), 2);
    assert_eq!(got.groups.len(), 2);
    assert_eq!(got.shared_states, 1);
    assert_eq!(got.adjacency_pages, 4);
    assert_eq!(got.status, EvidencePathStatus::Compatible);
    assert!(got.known_complete());
    assert_ne!(
        got.candidates[0].edge_indexes,
        got.candidates[1].edge_indexes
    );
}
