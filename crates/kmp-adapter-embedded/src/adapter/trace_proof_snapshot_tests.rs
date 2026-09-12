use super::*;
use crate::EmbeddedKernelStore;
use kmp_domain::*;

fn changes(epoch: &str) -> Vec<ProjectionMutation> {
    let mut changes = vec![];
    let at = if epoch == "old" {
        "2026-09-01T10:00:00Z"
    } else {
        "2026-09-02T10:00:00Z"
    };
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
                    .with_evidence(epoch)
                    .with_observed_at(at),
            },
        )));
    }
    let scope = MemoryDimensionIdentity::new("project:proof", "work", "one")
        .expect("valid proof fixture")
        .node_id();
    for id in ["a", "b"] {
        changes.push(ProjectionMutation::UpsertNodeRelation(Box::new(
            NodeRelationProjection {
                source_node_id: scope.clone(),
                target_node_id: id.into(),
                relation_type: "contains_entry".into(),
                explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("work")
                    .with_scope_id(scope.clone())
                    .with_observed_at(at),
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
        body: Default::default(),
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
        body: Default::default(),
    };
    let tx = store.begin_read().expect("valid snapshot fixture");
    let error = bounded_trace_search(&WrongBodies(TraceSnapshot(tx.as_ref())), &q)
        .expect_err("reordered batch must fail");
    assert!(error.to_string().contains("preserve requested slots"));
}

/// The writer commits precisely after graph/source selection, before any body
/// read. No sleeps, scheduling guesses or shared mutable query store.
struct CommitBeforeBodies<'a> {
    snapshot: TraceSnapshot<'a>,
    trigger: std::sync::mpsc::Sender<()>,
    committed: std::sync::mpsc::Receiver<()>,
}
impl TraceSnapshotReader for CommitBeforeBodies<'_> {
    fn node(&self, id: &str) -> Result<Option<NodeProjection>, PortError> {
        self.snapshot.node(id)
    }
    fn adjacency(&self, q: &AdjacencyRequest) -> Result<AdjacencyPage, PortError> {
        self.snapshot.adjacency(q)
    }
    fn bodies(&self, ids: &[String]) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        self.trigger.send(()).expect("valid proof fixture");
        self.committed
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("valid proof fixture");
        self.snapshot.bodies(ids)
    }
}

#[tokio::test]
async fn independent_writer_between_selected_graph_and_bodies_cannot_mix_either_mode() {
    for seek in [false, true] {
        let dir = tempfile::tempdir().expect("valid proof fixture");
        let reader = EmbeddedKernelStore::open(dir.path()).expect("valid proof fixture");
        let writer = EmbeddedKernelStore::open(dir.path()).expect("valid proof fixture");
        writer
            .apply_mutations(changes("old"))
            .await
            .expect("valid proof fixture");
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
            body: Default::default(),
        };
        let e = EvidencePathRequest {
            proof: true,
            about: q.about.clone(),
            from: q.from.clone(),
            roles: vec![EvidencePathRole {
                name: "dependency".into(),
                context: false,
                bindings: vec![],
                steps: vec![TraceRelationStep {
                    relation: MemoryRelationType::new("depends_on").expect("valid proof fixture"),
                    direction: RelationDirection::Outgoing,
                }],
            }],
            constants: Default::default(),
            temporal: Default::default(),
            limits: Default::default(),
            body: Default::default(),
        };
        let tx = reader.begin_read().expect("valid proof fixture");
        let (trigger, wait_trigger) = std::sync::mpsc::channel();
        let (committed, wait_commit) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            wait_trigger
                .recv_timeout(std::time::Duration::from_secs(10))
                .expect("valid proof fixture");
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("valid proof fixture")
                .block_on(writer.apply_mutations(changes("new")))
                .expect("valid proof fixture");
            committed.send(()).expect("valid proof fixture");
        });
        let snapshot = CommitBeforeBodies {
            snapshot: TraceSnapshot(tx.as_ref()),
            trigger,
            committed: wait_commit,
        };
        let (edges, proof) = if seek {
            let old = search_evidence_paths(&snapshot, &e).expect("valid proof fixture");
            (old.relations, old.proof.expect("valid proof fixture"))
        } else {
            let old = bounded_trace_search(&snapshot, &q).expect("valid proof fixture");
            (old.relations, old.proof.expect("valid proof fixture"))
        };
        thread.join().expect("valid proof fixture");
        assert_eq!(proof.complete_groups, [0]);
        assert!(
            edges
                .iter()
                .chain(&proof.supports)
                .all(|e| e.explanation.evidence() == Some("old")
                    && e.explanation.observed_at() == Some("2026-09-01T10:00:00Z"))
        );
        assert!(
            proof
                .objects
                .iter()
                .filter(|o| o.node.node_kind == "observation")
                .all(|o| o.coordinates.len() == 1
                    && o.coordinates[0].observed_at() == Some("2026-09-01T10:00:00Z"))
        );
        assert!(proof.objects.iter().all(|o| {
            o.node.properties["epoch"] == "old"
                && o.body.as_ref().is_some_and(|b| {
                    b.detail == "old" && b.content_hash == "old" && b.revision == 1
                })
        }));
        let live = if seek {
            reader
                .load_evidence_paths(&e)
                .await
                .expect("valid proof fixture")
                .proof
                .expect("valid proof fixture")
        } else {
            reader
                .load_bounded_trace(&q)
                .await
                .expect("valid proof fixture")
                .proof
                .expect("valid proof fixture")
        };
        assert!(live.objects.iter().all(|o| {
            o.node.properties["epoch"] == "new"
                && o.body.as_ref().is_some_and(|b| {
                    b.detail == "new" && b.content_hash == "new" && b.revision == 2
                })
        }));
    }
}
