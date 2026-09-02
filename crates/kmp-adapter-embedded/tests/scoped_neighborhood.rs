//! The dimension axis must narrow the load, not just the result.
//!
//! Every other test in this repository asserts what a read returns, and the
//! application filters the bundle after the fact, so a reader that ignored the
//! narrowing would pass all of them. This one asserts what the store was asked
//! to materialise — the only place the difference is visible.

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{
    GraphNeighborhoodReader, NeighborhoodRequest, NodeProjection, NodeRelationProjection,
    ProjectionMutation, ProjectionWriter, RelationExplanation, RelationSemanticClass,
};

const ABOUT: &str = "about:project";
const TIMELINE: &str = "about:project:dimension:timeline";
const CONVERSATION: &str = "about:project:dimension:conversation";

fn node(node_id: &str, kind: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: node_id.to_string(),
        node_kind: kind.to_string(),
        title: node_id.to_string(),
        summary: node_id.to_string(),
        status: "ACTIVE".to_string(),
        labels: Vec::new(),
        properties: Default::default(),
        provenance: None,
    })
}

fn contains(source: &str, target: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        relation_type: "contains_entry".to_string(),
        explanation: RelationExplanation::new(RelationSemanticClass::Structural),
    }))
}

async fn store_with_two_dimensions(path: &std::path::Path) -> EmbeddedKernelStore {
    let store = EmbeddedKernelStore::open(path).expect("store opens");
    store
        .apply_mutations(vec![
            node(ABOUT, "memory_anchor"),
            node(TIMELINE, "memory_dimension"),
            node(CONVERSATION, "memory_dimension"),
            node("project:entry:on-timeline", "memory_entry"),
            node("project:entry:in-conversation", "memory_entry"),
            contains(ABOUT, TIMELINE),
            contains(ABOUT, CONVERSATION),
            contains(TIMELINE, "project:entry:on-timeline"),
            contains(CONVERSATION, "project:entry:in-conversation"),
        ])
        .await
        .expect("projection writes");
    store
}

fn loaded(neighborhood: &kmp_domain::NodeNeighborhood) -> Vec<&str> {
    neighborhood
        .neighbors
        .iter()
        .map(|neighbor| neighbor.node_id.as_str())
        .collect()
}

#[tokio::test]
async fn an_unrequested_dimension_is_never_walked_into() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let store = store_with_two_dimensions(data_dir.path()).await;

    let narrowed = store
        .load_scoped_neighborhood(&NeighborhoodRequest::new(ABOUT, 3).with_scopes([TIMELINE]))
        .await
        .expect("the narrowed load succeeds")
        .expect("the about exists");
    let names = loaded(&narrowed);

    assert!(names.contains(&"project:entry:on-timeline"));
    assert!(
        !names.contains(&CONVERSATION),
        "the dimension nobody asked for was still walked: {names:?}"
    );
    assert!(
        !names.contains(&"project:entry:in-conversation"),
        "narrowing at the dimension must narrow everything it holds: {names:?}"
    );
}

#[tokio::test]
async fn a_request_that_names_no_scope_still_loads_everything() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let store = store_with_two_dimensions(data_dir.path()).await;

    let whole = store
        .load_neighborhood(ABOUT, 3)
        .await
        .expect("the unnarrowed load succeeds")
        .expect("the about exists");
    let names = loaded(&whole);

    assert!(names.contains(&"project:entry:on-timeline"));
    assert!(names.contains(&"project:entry:in-conversation"));
}

/// Narrowing is an optimisation, so the two paths must agree whenever the
/// request asks for everything. A store that answered them differently would
/// have made the hint a contract.
#[tokio::test]
async fn the_scoped_path_matches_the_plain_one_when_nothing_is_narrowed() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let store = store_with_two_dimensions(data_dir.path()).await;

    let plain = store.load_neighborhood(ABOUT, 3).await.expect("plain load");
    let scoped = store
        .load_scoped_neighborhood(&NeighborhoodRequest::new(ABOUT, 3))
        .await
        .expect("scoped load");

    assert_eq!(
        plain.map(|value| loaded(&value).join(",")),
        scoped.map(|value| loaded(&value).join(","))
    );
}
