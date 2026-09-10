use kmp_domain::{
    BundleRelationship, ProofDependencyGroup, RelationExplanation, RelationSemanticClass,
};
use std::collections::BTreeSet;

fn refs(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn edge(from: &str, rel: &str, to: &str) -> BundleRelationship {
    BundleRelationship::new(
        from,
        to,
        rel,
        RelationExplanation::new(RelationSemanticClass::Evidential)
            .with_optional_rationale(Some(
                "Declared connection in the supplied source".to_string(),
            ))
            .with_optional_evidence(Some("Source passage".to_string())),
    )
}

#[test]
fn follows_evidenced_connections_both_ways_without_equating_members() {
    let entries = refs(&["alias", "responsibility", "permit"]);
    let edges = vec![
        edge("alias", "same_entity_as", "responsibility"),
        edge("permit", "authorizes", "responsibility"),
    ];
    let group = ProofDependencyGroup::select("responsibility", &entries, &entries, &edges);
    assert_eq!(group.member_refs(), &["responsibility", "alias", "permit"]);
    assert_eq!(group.unavailable_in_selection(), 0);
    assert_eq!(group.omitted_by_limit(), 0);
    // Members are a neighborhood; authorizes is not rewritten to identity.
    assert_eq!(edges[1].relationship_type(), "authorizes");
}

#[test]
fn does_not_expose_ineligible_or_foreign_identities() {
    let scoped = refs(&["seed", "future-secret"]);
    let eligible = refs(&["seed"]);
    let edges = vec![
        edge("seed", "depends_on", "future-secret"),
        edge("seed", "depends_on", "foreign-secret"),
    ];
    let group = ProofDependencyGroup::select("seed", &scoped, &eligible, &edges);
    assert_eq!(group.member_refs(), &["seed"]);
    assert_eq!(group.unavailable_in_selection(), 1);
    assert!(!format!("{group:?}").contains("secret"));
}

#[test]
fn ignores_structural_and_unsubstantiated_edges() {
    let entries = refs(&["a", "b", "c", "d"]);
    let edges = vec![
        BundleRelationship::new(
            "a",
            "b",
            "contains",
            RelationExplanation::new(RelationSemanticClass::Structural)
                .with_optional_rationale(Some("Has rationale".into()))
                .with_optional_evidence(Some("Has source".into())),
        ),
        BundleRelationship::new(
            "a",
            "c",
            "supports",
            RelationExplanation::new(RelationSemanticClass::Evidential),
        ),
        edge("a", "supports", "d"),
    ];
    let group = ProofDependencyGroup::select("a", &entries, &entries, &edges);
    assert_eq!(group.member_refs(), &["a", "d"]);
}

#[test]
fn counts_unexpanded_frontier_at_two_hops_and_handles_cycles() {
    let entries = refs(&["a", "b", "c", "d"]);
    let edges = vec![
        edge("a", "depends_on", "b"),
        edge("b", "depends_on", "c"),
        edge("c", "depends_on", "d"),
        edge("b", "supports", "a"),
    ];
    let group = ProofDependencyGroup::select("a", &entries, &entries, &edges);
    assert_eq!(group.member_refs(), &["a", "b", "c"]);
    assert_eq!(group.omitted_by_limit(), 1);
}

#[test]
fn fanout_is_bounded_and_order_independent() {
    let mut edges = (0..12)
        .map(|n| edge("seed", "depends_on", &format!("member-{n:02}")))
        .collect::<Vec<_>>();
    let mut entries = refs(&["seed"]);
    entries.extend(edges.iter().map(|e| e.target_node_id().to_string()));
    let group = ProofDependencyGroup::select("seed", &entries, &entries, &edges);
    edges.reverse();
    assert_eq!(
        ProofDependencyGroup::select("seed", &entries, &entries, &edges),
        group
    );
    assert_eq!(group.member_refs().len(), 8);
    assert_eq!(group.omitted_by_limit(), 5);
}
