use super::*;
use crate::EmbeddedKernelStore;
use kmp_domain::*;

fn changes(epoch: &str) -> Vec<ProjectionMutation> {
    let mut changes = vec![];
    for (id, kind) in [
        ("a", "observation"),
        ("b", "observation"),
        ("source", "memory_evidence"),
    ] {
        changes.push(ProjectionMutation::UpsertNode(NodeProjection {
            node_id: id.into(),
            node_kind: kind.into(),
            title: id.into(),
            summary: epoch.into(),
            status: "ACTIVE".into(),
            labels: vec!["entry".into()],
            properties: [
                ("memory_about".into(), "project:proof".into()),
                ("epoch".into(), epoch.into()),
            ]
            .into(),
            provenance: None,
        }));
        changes.push(ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
            node_id: id.into(),
            detail: epoch.into(),
            content_hash: epoch.into(),
            revision: if epoch == "old" { 1 } else { 2 },
        }));
    }
    for (from, to, rel) in [
        ("a", "b", "depends_on"),
        ("source", "a", "supports"),
        ("source", "b", "supports"),
    ] {
        changes.push(ProjectionMutation::UpsertNodeRelation(Box::new(
            NodeRelationProjection {
                source_node_id: from.into(),
                target_node_id: to.into(),
                relation_type: rel.into(),
                explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                    .with_rationale(epoch)
                    .with_evidence(epoch),
            },
        )));
    }
    changes
}

#[tokio::test]
async fn selected_paths_sources_and_bodies_remain_one_state_after_independent_commit() {
    let dir = tempfile::tempdir().expect("valid snapshot fixture");
    let reader = EmbeddedKernelStore::open(dir.path()).expect("valid snapshot fixture");
    let writer = EmbeddedKernelStore::open(dir.path()).expect("valid snapshot fixture");
    writer
        .apply_mutations(changes("old"))
        .await
        .expect("valid snapshot fixture");
    let q = TraceSearchRequest {
        proof: true,
        about: "project:proof".into(),
        from: "a".into(),
        targets: ["b".into()].into(),
        direction: RelationDirection::Outgoing,
        relations: ["depends_on".into()].into(),
        follow: vec![],
        paths_per_target: 1,
        dimensions: Default::default(),
        select: None,
        limits: Default::default(),
        temporal: Default::default(),
    };
    let tx = reader.begin_read().expect("valid snapshot fixture");
    let snapshot = TraceSnapshot(tx.as_ref());
    let before = bounded_trace_search(&snapshot, &q).expect("valid snapshot fixture");
    writer
        .apply_mutations(changes("new"))
        .await
        .expect("valid snapshot fixture");
    assert_eq!(
        bounded_trace_search(&snapshot, &q).expect("valid snapshot fixture"),
        before
    );
    let live = reader
        .load_bounded_trace(&q)
        .await
        .expect("valid snapshot fixture");
    assert_ne!(live, before);
    assert!(
        live.proof
            .expect("valid snapshot fixture")
            .objects
            .iter()
            .all(|o| o.node.properties["epoch"] == "new"
                && o.body.as_ref().expect("valid snapshot fixture").detail == "new")
    );
}

struct WrongBodies<'a>(TraceSnapshot<'a>);
impl TraceSnapshotReader for WrongBodies<'_> {
    fn node(&self, id: &str) -> Result<Option<NodeProjection>, PortError> {
        self.0.node(id)
    }
    fn adjacency(&self, request: &AdjacencyRequest) -> Result<AdjacencyPage, PortError> {
        self.0.adjacency(request)
    }
    fn bodies(&self, ids: &[String]) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        let mut bodies = self.0.bodies(ids)?;
        bodies.reverse();
        Ok(bodies)
    }
}

#[tokio::test]
async fn a_body_port_reordering_slots_is_rejected_instead_of_mixing_sources() {
    let dir = tempfile::tempdir().expect("valid snapshot fixture");
    let store = EmbeddedKernelStore::open(dir.path()).expect("valid snapshot fixture");
    store
        .apply_mutations(changes("old"))
        .await
        .expect("valid snapshot fixture");
    let q = TraceSearchRequest {
        proof: true,
        about: "project:proof".into(),
        from: "a".into(),
        targets: ["b".into()].into(),
        direction: RelationDirection::Outgoing,
        relations: ["depends_on".into()].into(),
        follow: vec![],
        paths_per_target: 1,
        dimensions: Default::default(),
        select: None,
        limits: Default::default(),
        temporal: Default::default(),
    };
    let tx = store.begin_read().expect("valid snapshot fixture");
    let error = bounded_trace_search(&WrongBodies(TraceSnapshot(tx.as_ref())), &q)
        .expect_err("reordered batch must fail");
    assert!(error.to_string().contains("preserve requested slots"));
}
