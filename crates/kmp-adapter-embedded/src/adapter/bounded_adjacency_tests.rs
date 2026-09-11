use super::*;
use kmp_domain::{
    ProjectionMutation, ProjectionWriter, RelationExplanation, RelationSemanticClass,
};

fn edge(target: &str, evidence: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: "seed".into(),
        target_node_id: target.into(),
        relation_type: "depends_on".into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Causal)
            .with_optional_rationale(Some("Source declares this dependency.".into()))
            .with_optional_evidence(Some(evidence.into())),
    }))
}

#[tokio::test]
async fn transaction_local_pages_keep_one_snapshot_across_another_engines_commit() {
    let dir = tempfile::tempdir().expect("dir");
    let reader = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let writer = EmbeddedKernelStore::open(dir.path()).expect("independent writer");
    writer
        .apply_mutations(vec![edge("a", "old A"), edge("b", "old B")])
        .await
        .expect("seed");
    let tx = reader.begin_read().expect("snapshot");
    let request = AdjacencyRequest::new("seed", RelationDirection::Outgoing, 1).expect("request");
    let first = read_page(tx.as_ref(), &request).expect("first page");
    assert_eq!(first.edges[0].explanation.evidence(), Some("old A"));
    writer
        .apply_mutations(vec![edge("b", "new B")])
        .await
        .expect("interleaved commit");
    let request = request.with_after(first.next.expect("position"));
    let second = read_page(tx.as_ref(), &request).expect("same snapshot");
    assert_eq!(second.edges[0].explanation.evidence(), Some("old B"));
    drop(tx);
    let fresh = reader
        .read_adjacency(&request)
        .await
        .expect("new public call has a new snapshot");
    assert_eq!(fresh.edges[0].explanation.evidence(), Some("new B"));
}
