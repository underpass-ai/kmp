//! Directed storage pages are bounded before relation values are decoded.
use std::sync::Arc;

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{
    AdjacencyRequest, BoundedRelationReader, NodeProjection, NodeRelationProjection,
    ProjectionMutation, ProjectionWriter, RelationDirection, RelationExplanation, RelationPosition,
    RelationSemanticClass,
};

fn node(id: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: "memory_entry".into(),
        title: id.into(),
        summary: "Stored summary".into(),
        status: "ACTIVE".into(),
        labels: vec![],
        properties: Default::default(),
        provenance: None,
    })
}

fn edge(source: &str, target: &str, rel: &str) -> NodeRelationProjection {
    NodeRelationProjection {
        source_node_id: source.into(),
        target_node_id: target.into(),
        relation_type: rel.into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Causal)
            .with_optional_rationale(Some("The source explicitly requires the target.".into()))
            .with_optional_evidence(Some(
                "Register: A requires B; preserve this exact source.".into(),
            )),
    }
}

async fn fixture(path: &std::path::Path) -> Arc<EmbeddedKernelStore> {
    let store = Arc::new(EmbeddedKernelStore::open(path).expect("store"));
    let mut mutations = vec![node("seed")];
    for i in 0..65 {
        let target = format!("neighbor-{i:03}");
        mutations.extend([
            node(&target),
            ProjectionMutation::UpsertNodeRelation(Box::new(edge("seed", &target, "depends_on"))),
            ProjectionMutation::UpsertNodeRelation(Box::new(edge(&target, "seed", "supports"))),
        ]);
    }
    mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(edge(
        "seed",
        "neighbor-000",
        "verified_by",
    ))));
    mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(edge(
        "seed",
        "seed",
        "depends_on",
    ))));
    store.apply_mutations(mutations).await.expect("seed");
    store
}

#[tokio::test]
async fn pages_preserve_direction_sources_and_complete_key_order_without_duplicates() {
    let dir = tempfile::tempdir().expect("dir");
    let store = fixture(dir.path()).await;
    for direction in [RelationDirection::Outgoing, RelationDirection::Incoming] {
        let mut request = AdjacencyRequest::new("seed", direction, 7).expect("request");
        let mut found = Vec::new();
        loop {
            // Arc and reference wrappers must retain the strict bounded port.
            let reader = &store;
            let page = reader.read_adjacency(&request).await.expect("page");
            assert!(page.edges.len() <= 7);
            for link in &page.edges {
                assert_eq!(
                    link.explanation.evidence(),
                    Some("Register: A requires B; preserve this exact source.")
                );
                match direction {
                    RelationDirection::Outgoing => assert_eq!(link.source_node_id, "seed"),
                    RelationDirection::Incoming => assert_eq!(link.target_node_id, "seed"),
                }
            }
            found.extend(page.edges);
            if page.exhausted {
                assert!(page.next.is_none());
                break;
            }
            request = request.with_after(page.next.expect("full page has a key"));
        }
        let mut expected = (0..65)
            .map(|i| {
                let neighbor = format!("neighbor-{i:03}");
                match direction {
                    RelationDirection::Outgoing => edge("seed", &neighbor, "depends_on"),
                    RelationDirection::Incoming => edge(&neighbor, "seed", "supports"),
                }
            })
            .collect::<Vec<_>>();
        if direction == RelationDirection::Outgoing {
            expected.insert(1, edge("seed", "neighbor-000", "verified_by"));
        }
        expected.push(edge("seed", "seed", "depends_on"));
        assert_eq!(found, expected);
    }
}

#[tokio::test]
async fn a_full_final_page_is_not_misreported_as_an_exhausted_node() {
    let dir = tempfile::tempdir().expect("dir");
    let store = fixture(dir.path()).await;
    let request = AdjacencyRequest::new("seed", RelationDirection::Incoming, 66).expect("request");
    let first = store.read_adjacency(&request).await.expect("page");
    assert_eq!(first.edges.len(), 66);
    assert!(!first.exhausted);
    let end = store
        .read_adjacency(&request.with_after(first.next.expect("position")))
        .await
        .expect("end");
    assert!(end.exhausted);
    assert!(end.edges.is_empty());
    assert!(end.next.is_none());
}

#[tokio::test]
async fn keyset_uses_the_relation_as_well_as_neighbor_without_trimming_identity() {
    let dir = tempfile::tempdir().expect("dir");
    let store = fixture(dir.path()).await;
    let request = AdjacencyRequest::new("seed", RelationDirection::Outgoing, 1)
        .expect("request")
        .with_after(RelationPosition {
            neighbor: "neighbor-000".into(),
            relation: "depends_on".into(),
        });
    let page = store.read_adjacency(&request).await.expect("page");
    assert_eq!(
        page.edges,
        vec![edge("seed", "neighbor-000", "verified_by")]
    );
    let absent = AdjacencyRequest::new(" seed", RelationDirection::Outgoing, 1).expect("opaque id");
    let page = store
        .read_adjacency(&absent)
        .await
        .expect("absent adjacency");
    assert!(page.edges.is_empty());
    assert!(page.exhausted);
}

#[test]
fn zero_budget_and_empty_node_are_rejected_before_storage() {
    assert!(AdjacencyRequest::new("seed", RelationDirection::Outgoing, 0).is_err());
    assert!(AdjacencyRequest::new(" ", RelationDirection::Outgoing, 1).is_err());
}
