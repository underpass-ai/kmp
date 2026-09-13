use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{
    MemoryAboutIndexReader, MemoryDimensionIdentity, NodeProjection, NodeRelationProjection,
    ProjectionMutation, ProjectionWriter, RelationExplanation, RelationSemanticClass,
};
use std::collections::BTreeMap;

fn directory() -> tempfile::TempDir {
    let scratch =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/performance-775-tests");
    std::fs::create_dir_all(&scratch).expect("scratch directory");
    tempfile::tempdir_in(scratch).expect("isolated fixture directory")
}

fn node(id: &str, kind: &str, dimension_kind: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: kind.into(),
        title: id.into(),
        summary: id.into(),
        status: "ACTIVE".into(),
        labels: vec![],
        properties: BTreeMap::from([("dimension_kind".into(), dimension_kind.into())]),
        provenance: None,
    })
}

fn edge(from: &str, to: &str, kind: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: from.into(),
        target_node_id: to.into(),
        relation_type: kind.into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Structural),
    }))
}

async fn lookup(store: &EmbeddedKernelStore, terms: &[&str]) -> Vec<String> {
    store
        .list_memory_abouts_by_dimensions(&terms.iter().map(|s| (*s).into()).collect::<Vec<_>>())
        .await
        .expect("dimension lookup")
}

#[tokio::test]
async fn dimension_lookup_preserves_full_ids_values_kinds_and_sorted_union() {
    let dir = directory();
    let store = EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds");
    let a = MemoryDimensionIdentity::new("project:a", "task", "Nébula / a:b")
        .expect("fixture operation succeeds")
        .node_id();
    let b = MemoryDimensionIdentity::new("project:b", "topic", "Nébula / a:b")
        .expect("fixture operation succeeds")
        .node_id();
    store
        .apply_mutations(vec![
            node("project:b", "memory_anchor", ""),
            node("project:a", "memory_anchor", ""),
            node(&a, "memory_dimension", "task"),
            node(&b, "memory_dimension", "topic"),
            edge("project:a", &a, "has_dimension"),
            edge("project:b", &b, "has_dimension"),
            node("ordinary", "memory_entry", "task"),
            edge("project:b", "ordinary", "has_dimension"),
            node("unlinked", "memory_dimension", "task"),
            edge("project:b", "unlinked", "other"),
            edge("not-an-anchor", &a, "has_dimension"),
        ])
        .await
        .expect("fixture operation succeeds");
    assert_eq!(lookup(&store, &[&a]).await, ["project:a"]);
    assert_eq!(
        lookup(&store, &["Nébula / a:b"]).await,
        ["project:a", "project:b"]
    );
    assert_eq!(
        lookup(&store, &["topic", "task", "task"]).await,
        ["project:a", "project:b"]
    );
    assert_eq!(lookup(&store, &["task"]).await, ["project:a"]);
    assert!(lookup(&store, &["absent"]).await.is_empty());
    assert!(lookup(&store, &[]).await.is_empty());
}

#[tokio::test]
async fn dimension_lookup_crosses_pages_without_duplicates_or_missing_endpoints() {
    let dir = directory();
    let store = EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds");
    let mut mutations = vec![node("project:a", "memory_anchor", "")];
    for i in 0..600 {
        let id = format!("dimension:{i:04}");
        mutations.push(node(&id, "memory_dimension", "topic"));
        mutations.push(edge("project:a", &id, "has_dimension"));
    }
    // A page boundary may fall inside one anchor, and another anchor may share its label.
    mutations.extend([
        node("project:z", "memory_anchor", ""),
        edge("project:z", "dimension:0599", "has_dimension"),
        edge("project:z", "missing-placeholder", "has_dimension"),
    ]);
    store
        .apply_mutations(mutations)
        .await
        .expect("fixture operation succeeds");
    assert_eq!(
        lookup(&store, &["dimension:0599"]).await,
        ["project:a", "project:z"]
    );
    assert_eq!(lookup(&store, &["topic"]).await, ["project:a", "project:z"]);
    assert!(lookup(&store, &["missing-placeholder"]).await.is_empty());
}

#[tokio::test]
async fn dimension_lookup_and_graph_keep_the_pinned_state_across_peer_mutations() {
    let dir = directory();
    let store = EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds");
    let peer = EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds");
    store
        .apply_mutations(vec![
            node("project:a", "memory_anchor", ""),
            node("label", "memory_dimension", "task"),
            edge("project:a", "label", "has_dimension"),
        ])
        .await
        .expect("fixture operation succeeds");
    let snapshot = store
        .read_snapshot()
        .await
        .expect("fixture operation succeeds");
    peer.apply_mutations(vec![
        node("label", "memory_dimension", "topic"),
        node("project:b", "memory_anchor", ""),
        edge("project:b", "label", "has_dimension"),
    ])
    .await
    .expect("fixture operation succeeds");
    assert_eq!(lookup(&snapshot, &["task"]).await, ["project:a"]);
    assert!(lookup(&snapshot, &["topic"]).await.is_empty());
    assert_eq!(lookup(&store, &["topic"]).await, ["project:a", "project:b"]);
    peer.apply_mutations(vec![
        ProjectionMutation::RemoveNodeRelation {
            source_node_id: "project:a".into(),
            target_node_id: "label".into(),
            relation_type: "has_dimension".into(),
        },
        node("project:b", "memory_entry", ""),
    ])
    .await
    .expect("fixture operation succeeds");
    assert!(lookup(&store, &["topic"]).await.is_empty());
    assert_eq!(lookup(&snapshot, &["task"]).await, ["project:a"]);
    drop(snapshot);
    peer.apply_mutations(vec![
        node("project:b", "memory_anchor", ""),
        node("label", "memory_entry", "topic"),
    ])
    .await
    .expect("fixture operation succeeds");
    assert!(lookup(&store, &["topic"]).await.is_empty());
    peer.apply_mutations(vec![node("label", "memory_dimension", "topic")])
        .await
        .expect("fixture operation succeeds");
    assert_eq!(lookup(&store, &["topic"]).await, ["project:b"]);
    assert_eq!(
        lookup(
            &EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds"),
            &["topic"]
        )
        .await,
        ["project:b"]
    );
}
