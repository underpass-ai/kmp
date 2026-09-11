use super::*;
use crate::EmbeddedKernelStore;
use kmp_domain::{
    GraphNeighborhoodReader, NodeRelationProjection, ProjectionMutation, ProjectionWriter,
    RelationDirection, RelationExplanation, RelationSemanticClass, TraceSearchLimits,
    TraceSearchRequest, TraceSearchStop, bounded_trace_search,
};

fn node(id: &str, about: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: "observation".into(),
        title: id.into(),
        summary: id.into(),
        status: "ACTIVE".into(),
        labels: vec!["entry".into()],
        properties: [("memory_about".into(), about.into())].into(),
        provenance: None,
    })
}

fn edge(from: &str, to: &str, source: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: from.into(),
        target_node_id: to.into(),
        relation_type: "depends_on".into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Causal)
            .with_optional_rationale(Some("The register declares a dependency.".into()))
            .with_optional_evidence(Some(source.into())),
    }))
}

#[tokio::test]
async fn ownership_and_all_route_explanations_share_one_snapshot_during_independent_commit() {
    let dir = tempfile::tempdir().expect("temporary store");
    let reader = EmbeddedKernelStore::open(dir.path()).expect("open fixture store");
    let writer = EmbeddedKernelStore::open(dir.path()).expect("open fixture store");
    writer
        .apply_mutations(vec![
            node("a", "project:test"),
            node("b", "project:test"),
            node("c", "project:test"),
            edge("a", "b", "old AB"),
            edge("b", "c", "old BC"),
        ])
        .await
        .expect("fixture write or bounded read");
    let tx = reader.begin_read().expect("read snapshot");
    let snapshot = TraceSnapshot(tx.as_ref());
    assert!(
        snapshot
            .node("a")
            .expect("fixture write or bounded read")
            .is_some()
    );
    writer
        .apply_mutations(vec![edge("b", "c", "new BC"), node("b", "project:foreign")])
        .await
        .expect("fixture write or bounded read");
    let request = TraceSearchRequest {
        about: "project:test".into(),
        from: "a".into(),
        targets: ["c".into()].into(),
        direction: RelationDirection::Outgoing,
        dimensions: Default::default(),
        select: None,
        follow: vec![],
        paths_per_target: 1,
        relations: Default::default(),
        limits: TraceSearchLimits::default(),
        temporal: Default::default(),
    };
    let result = bounded_trace_search(&snapshot, &request).expect("fixture write or bounded read");
    assert_eq!(result.stop, TraceSearchStop::TargetsReached);
    assert_eq!(result.relations[1].explanation.evidence(), Some("old BC"));
    drop(tx);
    let fresh = reader
        .load_bounded_trace(&request)
        .await
        .expect("fixture write or bounded read");
    assert_eq!(fresh.stop, TraceSearchStop::FrontierExhausted);
    assert!(
        fresh.relations.is_empty(),
        "new ownership excludes the entire foreign path"
    );
}

#[tokio::test]
async fn coordinate_admission_and_proof_share_one_snapshot_during_retiming() {
    use kmp_domain::{MemoryDimensionIdentity, TemporalAxis, TemporalCursor, TemporalSelection};
    let dir = tempfile::tempdir().expect("directory");
    let reader = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let writer = EmbeddedKernelStore::open(dir.path()).expect("writer");
    let lane = MemoryDimensionIdentity::new("project:test", "work", "one")
        .expect("label")
        .node_id();
    let coordinate = |id: &str, at: &str| {
        ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
            source_node_id: lane.clone(),
            target_node_id: id.into(),
            relation_type: "contains_entry".into(),
            explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                .with_dimension("work")
                .with_scope_id(lane.clone())
                .with_observed_at(at),
        }))
    };
    writer
        .apply_mutations(vec![
            node("a", "project:test"),
            node("b", "project:test"),
            coordinate("a", "2026-09-11T09:00:00Z"),
            coordinate("b", "2026-09-11T09:00:00Z"),
            edge("a", "b", "old proof"),
        ])
        .await
        .expect("initial");
    let tx = reader.begin_read().expect("snapshot");
    let snapshot = TraceSnapshot(tx.as_ref());
    assert!(snapshot.node("a").expect("fix snapshot").is_some());
    writer
        .apply_mutations(vec![
            coordinate("b", "2026-09-11T11:00:00Z"),
            edge("a", "b", "new proof"),
        ])
        .await
        .expect("later commit");
    let request = TraceSearchRequest {
        about: "project:test".into(),
        from: "a".into(),
        targets: ["b".into()].into(),
        direction: RelationDirection::Outgoing,
        dimensions: Default::default(),
        select: None,
        follow: vec![],
        paths_per_target: 1,
        relations: Default::default(),
        limits: Default::default(),
        temporal: TemporalSelection::as_of(
            TemporalCursor::time("2026-09-11T10:00:00Z").expect("cut"),
            TemporalAxis::Observed,
        )
        .expect("selection"),
    };
    let result = bounded_trace_search(&snapshot, &request).expect("old snapshot");
    assert_eq!(result.stop, TraceSearchStop::TargetsReached);
    assert_eq!(
        result.relations[0].explanation.evidence(),
        Some("old proof")
    );
    drop(tx);
    assert!(
        reader
            .load_bounded_trace(&request)
            .await
            .expect("fresh snapshot")
            .routes
            .is_empty()
    );
}
