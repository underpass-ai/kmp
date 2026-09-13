use kmp_domain::*;
use std::{cell::RefCell, collections::BTreeMap};

struct Snapshot {
    nodes: BTreeMap<String, NodeProjection>,
    edges: Vec<NodeRelationProjection>,
    reads: RefCell<Vec<String>>,
}
impl Snapshot {
    fn new(coordinates: usize) -> Self {
        let nodes = ["a", "b", "foreign"]
            .into_iter()
            .map(|id| {
                (
                    id.into(),
                    NodeProjection {
                        node_id: id.into(),
                        node_kind: "observation".into(),
                        title: id.into(),
                        summary: format!("Header {id}"),
                        status: "ACTIVE".into(),
                        labels: vec!["entry".into()],
                        properties: [(
                            "memory_about".into(),
                            if id == "foreign" {
                                "other"
                            } else {
                                "project:test"
                            }
                            .into(),
                        )]
                        .into(),
                        provenance: None,
                    },
                )
            })
            .collect();
        let edges = (0..coordinates)
            .map(|i| NodeRelationProjection {
                source_node_id: format!("dimension:{i:04}"),
                target_node_id: "a".into(),
                relation_type: "contains_entry".into(),
                explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("task")
                    .with_scope_id(format!("task:{i}"))
                    .with_observed_at("2026-09-13T01:00:00.123456789Z")
                    .with_method("kmp_relabel")
                    .with_rationale("The source explicitly assigns this label."),
            })
            .collect();
        Self {
            nodes,
            edges,
            reads: Default::default(),
        }
    }
}
impl TraceSnapshotReader for Snapshot {
    fn node(&self, id: &str) -> Result<Option<NodeProjection>, PortError> {
        self.reads.borrow_mut().push(id.into());
        Ok(self.nodes.get(id).cloned())
    }
    fn adjacency(&self, q: &AdjacencyRequest) -> Result<AdjacencyPage, PortError> {
        assert_eq!(q.direction(), RelationDirection::Incoming);
        assert_eq!(q.relation_type(), Some("contains_entry"));
        let rows: Vec<_> = self
            .edges
            .iter()
            .filter(|e| {
                e.target_node_id == q.node_id()
                    && q.after().is_none_or(|p| e.source_node_id > p.neighbor)
            })
            .collect();
        let exhausted = rows.len() <= q.limit() as usize;
        let edges: Vec<_> = rows.into_iter().take(q.limit() as usize).cloned().collect();
        let next = if exhausted {
            None
        } else {
            edges.last().map(|e| RelationPosition {
                neighbor: e.source_node_id.clone(),
                relation: e.relation_type.clone(),
            })
        };
        Ok(AdjacencyPage {
            edges,
            next,
            exhausted,
        })
    }
}
fn request(refs: &[&str], max_edges: u32) -> MemoryNodesRequest {
    MemoryNodesRequest {
        expect_snapshot: None,
        about: "project:test".into(),
        refs: refs.iter().map(|s| s.to_string()).collect(),
        max_edges,
    }
}

#[test]
fn shares_point_reads_and_preserves_full_coordinate_provenance_without_reading_bodies() {
    let snapshot = Snapshot::new(70);
    let result = read_memory_nodes(
        &snapshot,
        &request(&["a", "b", "a", "missing", "foreign"], 2048),
    )
    .expect("batch");
    assert_eq!(
        result
            .nodes
            .iter()
            .map(|n| n.node.node_id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(result.missing, vec!["missing", "foreign"]);
    assert_eq!(
        snapshot.reads.borrow().as_slice(),
        &["a", "b", "missing", "foreign"]
    );
    assert_eq!(result.nodes[0].coordinates.len(), 70);
    assert!(result.nodes.iter().all(|n| n.coordinates_complete));
    let c = &result.nodes[0].coordinates[0];
    assert_eq!(c.observed_at(), Some("2026-09-13T01:00:00.123456789Z"));
    assert_eq!(c.origin().method(), Some("kmp_relabel"));
    assert_eq!(
        c.origin().rationale(),
        Some("The source explicitly assigns this label.")
    );
    assert_eq!(result.scanned_edges, 70);
    assert_eq!(result.stop, None);
}

#[test]
fn common_edge_budget_reports_partial_coordinates_and_unread_refs_separately() {
    let result = read_memory_nodes(&Snapshot::new(70), &request(&["a", "b", "missing"], 40))
        .expect("bounded batch");
    assert_eq!(result.scanned_edges, 40);
    assert_eq!(result.nodes[0].coordinates.len(), 40);
    assert!(!result.nodes[0].coordinates_complete);
    assert_eq!(result.stop, Some(TraceSearchStop::EdgeBudget));
    assert_eq!(result.omitted, vec!["b", "missing"]);
    assert!(result.missing.is_empty(), "unread is not absent");
}

#[test]
fn rejects_invalid_batches_before_reading() {
    let snapshot = Snapshot::new(1);
    for q in [
        request(&[], 10),
        request(&[" a"], 10),
        request(&["a"], 0),
        request(&["a"], 32769),
        request(&["a"; 65], 10),
    ] {
        assert!(read_memory_nodes(&snapshot, &q).is_err());
    }
    assert!(snapshot.reads.borrow().is_empty());
}
