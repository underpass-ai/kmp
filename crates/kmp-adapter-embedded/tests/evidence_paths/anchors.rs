use super::support::*;
use kmp_domain::*;

fn reference(at: u32, name: &str) -> EvidencePathBinding {
    EvidencePathBinding::Reference {
        at,
        name: name.into(),
    }
}

fn contextual(name: &str, relation: &str, bindings: Vec<EvidencePathBinding>) -> EvidencePathRole {
    let mut role = role(name, &[relation], bindings);
    role.context = true;
    role
}

#[tokio::test]
async fn context_anchor_equality_keeps_longer_authorized_action_and_separate_witnesses() {
    let (_dir, store) = store(vec![
        node("s"),
        node("bridge"),
        node("a"),
        node("b"),
        node("v"),
        node("p"),
        edge("s", "b", "uses_background", Some(EARLY)),
        edge("s", "bridge", "uses_background", Some(EARLY)),
        edge("bridge", "a", "uses_background", Some(EARLY)),
        // The same report mentions both executions: witness equality cannot
        // distinguish their anchors at the convergent report state.
        edge("a", "v", "verified_by", Some(EARLY)),
        edge("b", "v", "verified_by", Some(EARLY)),
        edge("p", "a", "authorizes", Some(EARLY)),
    ])
    .await;
    let verification = contextual("verification", "verified_by", vec![reference(0, "action")]);
    let mut permission = contextual("permission", "authorizes", vec![reference(0, "action")]);
    permission.steps[0].direction = RelationDirection::Incoming;
    let found = store
        .load_evidence_paths(&request(vec![verification, permission]))
        .await
        .expect("seek");
    assert_eq!(found.status, EvidencePathStatus::ReviewRequired);
    assert_eq!(found.stop, TraceSearchStop::FrontierExhausted);
    assert_eq!(found.groups.len(), 1);
    let group = &found.groups[0];
    assert_eq!(group.bindings.domains["action"], ["a".into()].into());
    let v = &found.candidates[group.candidate_indexes[0] as usize];
    assert_eq!(v.nodes, ["s", "bridge", "a", "v"]);
    assert_eq!(v.context_hops, 2);
    assert_eq!(found.candidates.iter().filter(|c| c.role == 0).count(), 2);
    assert!(!found.known_complete());
}

#[tokio::test]
async fn disjoint_action_anchors_cannot_form_a_group_even_with_a_shared_witness() {
    let (_dir, store) = store(vec![
        node("s"),
        node("a"),
        node("b"),
        node("p"),
        edge("s", "a", "uses_background", Some(EARLY)),
        edge("s", "b", "uses_background", Some(EARLY)),
        edge("a", "p", "verified_by", Some(EARLY)),
        edge("p", "b", "authorizes", Some(EARLY)),
    ])
    .await;
    let verification = contextual(
        "v",
        "verified_by",
        vec![reference(0, "action"), reference(1, "person")],
    );
    let mut permission = contextual(
        "p",
        "authorizes",
        vec![reference(0, "action"), reference(1, "person")],
    );
    permission.steps[0].direction = RelationDirection::Incoming;
    let found = store
        .load_evidence_paths(&request(vec![verification, permission]))
        .await
        .expect("seek");
    assert_eq!(found.candidates.len(), 2);
    assert!(found.missing_roles.is_empty());
    assert!(found.groups.is_empty());
    assert_eq!(found.status, EvidencePathStatus::IncompatibleObligations);
}

#[tokio::test]
async fn a_witness_can_bind_another_roles_context_anchor_without_binding_bridges() {
    let (_dir, store) = store(vec![
        node("s"),
        node("bridge"),
        node("a"),
        node("v"),
        edge("s", "a", "depends_on", Some(EARLY)),
        edge("s", "bridge", "uses_background", Some(EARLY)),
        edge("bridge", "a", "uses_background", Some(EARLY)),
        edge("a", "v", "verified_by", Some(EARLY)),
    ])
    .await;
    let dependency = role("dependency", &["depends_on"], vec![reference(1, "action")]);
    let verification = contextual("verification", "verified_by", vec![reference(0, "action")]);
    let found = store
        .load_evidence_paths(&request(vec![dependency, verification]))
        .await
        .expect("seek");
    assert_eq!(found.groups.len(), 1);
    assert_eq!(
        found.groups[0].bindings.domains["action"],
        ["a".into()].into()
    );
}

#[tokio::test]
async fn explicit_prefix_anchor_binds_after_the_prefix_before_the_main_step() {
    let (_dir, store) = store(vec![
        node("s"),
        node("a"),
        node("v"),
        node("p"),
        edge("s", "a", "uses_background", Some(EARLY)),
        edge("a", "v", "verified_by", Some(EARLY)),
        edge("p", "a", "authorizes", Some(EARLY)),
    ])
    .await;
    let verification = role(
        "v",
        &["uses_background", "verified_by"],
        vec![reference(1, "action")],
    );
    let mut permission = contextual("p", "authorizes", vec![reference(0, "action")]);
    permission.steps[0].direction = RelationDirection::Incoming;
    let found = store
        .load_evidence_paths(&request(vec![verification, permission]))
        .await
        .expect("seek");
    assert_eq!(found.groups.len(), 1);
    assert_eq!(
        found.groups[0].bindings.domains["action"],
        ["a".into()].into()
    );
}
