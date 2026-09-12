use kmp_domain::*;
use std::{cell::RefCell, collections::BTreeMap};

struct Reader {
    nodes: BTreeMap<String, NodeProjection>,
    edges: Vec<NodeRelationProjection>,
    body_calls: RefCell<Vec<Vec<String>>>,
    node_calls: RefCell<BTreeMap<String, u32>>,
}
impl Reader {
    fn fixture() -> Self {
        let nodes = ["a", "b", "c", "source"]
            .into_iter()
            .map(|id| {
                (
                    id.into(),
                    NodeProjection {
                        node_id: id.into(),
                        node_kind: if id == "source" {
                            "memory_evidence"
                        } else {
                            "observation"
                        }
                        .into(),
                        title: id.into(),
                        summary: id.into(),
                        status: "ACTIVE".into(),
                        labels: if id == "source" {
                            vec![]
                        } else {
                            vec!["entry".into()]
                        },
                        properties: [("memory_about".into(), "project:test".into())].into(),
                        provenance: None,
                    },
                )
            })
            .collect();
        let edges = [
            ("a", "b", "depends_on"),
            ("a", "c", "depends_on"),
            ("source", "a", "supports"),
            ("source", "b", "supports"),
            ("source", "c", "supports"),
        ]
        .into_iter()
        .map(|(a, b, r)| NodeRelationProjection {
            source_node_id: a.into(),
            target_node_id: b.into(),
            relation_type: r.into(),
            explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                .with_rationale("The register declares this link.")
                .with_evidence(format!("{a} {r} {b}")),
        })
        .collect();
        Self {
            nodes,
            edges,
            body_calls: Default::default(),
            node_calls: Default::default(),
        }
    }
}
impl TraceSnapshotReader for Reader {
    fn node(&self, id: &str) -> Result<Option<NodeProjection>, PortError> {
        *self.node_calls.borrow_mut().entry(id.into()).or_default() += 1;
        Ok(self.nodes.get(id).cloned())
    }
    fn adjacency(&self, q: &AdjacencyRequest) -> Result<AdjacencyPage, PortError> {
        let mut rows: Vec<_> = self
            .edges
            .iter()
            .filter_map(|e| {
                let (from, to) = match q.direction() {
                    RelationDirection::Outgoing => (&e.source_node_id, &e.target_node_id),
                    RelationDirection::Incoming => (&e.target_node_id, &e.source_node_id),
                };
                if from != q.node_id() || q.relation_type().is_some_and(|r| r != e.relation_type) {
                    return None;
                }
                let key = (to.clone(), e.relation_type.clone());
                if q.after()
                    .is_some_and(|p| key <= (p.neighbor.clone(), p.relation.clone()))
                {
                    return None;
                }
                Some((key, e.clone()))
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows.truncate(q.limit() as usize);
        let exhausted = rows.len() < q.limit() as usize;
        let next = if exhausted {
            None
        } else {
            rows.last().map(|((n, r), _)| RelationPosition {
                neighbor: n.clone(),
                relation: r.clone(),
            })
        };
        Ok(AdjacencyPage {
            edges: rows.into_iter().map(|(_, e)| e).collect(),
            next,
            exhausted,
        })
    }
    fn bodies(&self, ids: &[String]) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        self.body_calls.borrow_mut().push(ids.to_vec());
        Ok(ids
            .iter()
            .map(|id| {
                Some(NodeDetailProjection {
                    node_id: id.clone(),
                    detail: format!("Canonical π {id}"),
                    content_hash: format!("hash:{id}"),
                    revision: 9,
                })
            })
            .collect())
    }
}
fn trace() -> TraceSearchRequest {
    TraceSearchRequest {
        proof: true,
        about: "project:test".into(),
        from: "a".into(),
        targets: ["b".into(), "c".into()].into(),
        direction: RelationDirection::Outgoing,
        relations: ["depends_on".into()].into(),
        follow: vec![],
        paths_per_target: 1,
        dimensions: Default::default(),
        select: None,
        temporal: Default::default(),
        limits: TraceSearchLimits {
            nodes: 5,
            ..Default::default()
        },
        body: Default::default(),
    }
}
fn seek() -> EvidencePathRequest {
    EvidencePathRequest {
        proof: true,
        about: "project:test".into(),
        from: "a".into(),
        roles: vec![EvidencePathRole {
            name: "cause".into(),
            context: false,
            bindings: vec![],
            steps: vec![TraceRelationStep {
                relation: MemoryRelationType::new("depends_on").expect("valid proof fixture"),
                direction: RelationDirection::Outgoing,
            }],
        }],
        constants: Default::default(),
        temporal: Default::default(),
        limits: trace().limits,
        body: Default::default(),
    }
}

#[test]
fn both_modes_share_one_source_and_one_body_batch_with_sufficient_capacity() {
    for seed in [false, true] {
        let r = Reader::fixture();
        let proof = if seed {
            search_evidence_paths(&r, &seek())
                .expect("valid proof fixture")
                .proof
                .expect("valid proof fixture")
        } else {
            bounded_trace_search(&r, &trace())
                .expect("valid proof fixture")
                .proof
                .expect("valid proof fixture")
        };
        assert_eq!(proof.complete_groups, [0, 1]);
        assert!(proof.incomplete_groups.is_empty());
        assert!(proof.stop.is_none());
        assert_eq!(r.node_calls.borrow()["source"], 1);
        assert_eq!(*r.body_calls.borrow(), vec![vec!["a", "b", "c", "source"]]);
        assert_eq!(proof.supports.len(), 3);
        assert!(
            proof
                .objects
                .iter()
                .all(|o| o.body.as_ref().expect("valid proof fixture").revision == 9)
        );
    }
}

struct WithoutBodies(Reader);
impl TraceSnapshotReader for WithoutBodies {
    fn node(&self, id: &str) -> Result<Option<NodeProjection>, PortError> {
        self.0.node(id)
    }
    fn adjacency(&self, q: &AdjacencyRequest) -> Result<AdjacencyPage, PortError> {
        self.0.adjacency(q)
    }
}
#[test]
fn nonproof_readers_still_work_and_proof_is_explicitly_unsupported() {
    let r = WithoutBodies(Reader::fixture());
    let mut q = trace();
    q.proof = false;
    assert!(
        bounded_trace_search(&r, &q)
            .expect("valid proof fixture")
            .proof
            .is_none()
    );
    q.proof = true;
    assert!(matches!(
        bounded_trace_search(&r, &q),
        Err(PortError::Unavailable(_))
    ));
    let mut q = seek();
    q.proof = false;
    assert!(
        search_evidence_paths(&r, &q)
            .expect("valid proof fixture")
            .proof
            .is_none()
    );
    q.proof = true;
    assert!(matches!(
        search_evidence_paths(&r, &q),
        Err(PortError::Unavailable(_))
    ));
}

#[test]
fn missing_sources_at_capacity_are_not_confused_with_unread_sources() {
    let mut r = Reader::fixture();
    r.nodes.remove("source");
    let proof = bounded_trace_search(&r, &trace())
        .expect("valid proof fixture")
        .proof
        .expect("valid proof fixture");
    assert_eq!(proof.missing_refs, ["source"]);
    assert_eq!(proof.incomplete_entries, ["a", "b", "c"]);
    assert_eq!(*r.body_calls.borrow(), vec![vec!["a", "b", "c"]]);
    let mut q = trace();
    q.limits.nodes = 3;
    let proof = bounded_trace_search(&r, &q)
        .expect("valid proof fixture")
        .proof
        .expect("valid proof fixture");
    assert_eq!(proof.stop, Some(TraceSearchStop::NodeBudget));
    assert!(proof.missing_refs.is_empty());
    assert!(proof.complete_groups.is_empty());
}

#[test]
fn exact_node_capacity_remains_partial_without_probing_unreserved_endpoints() {
    for seed in [false, true] {
        let r = Reader::fixture();
        let proof = if seed {
            let mut q = seek();
            q.limits.nodes = 4;
            search_evidence_paths(&r, &q)
                .expect("valid proof fixture")
                .proof
                .expect("proof")
        } else {
            let mut q = trace();
            q.limits.nodes = 4;
            bounded_trace_search(&r, &q)
                .expect("valid proof fixture")
                .proof
                .expect("proof")
        };
        assert_eq!(proof.stop, Some(TraceSearchStop::NodeBudget));
        assert!(proof.complete_groups.is_empty());
        assert_eq!(proof.incomplete_groups, [0, 1]);
        assert!(proof.missing_refs.is_empty());
    }
}
