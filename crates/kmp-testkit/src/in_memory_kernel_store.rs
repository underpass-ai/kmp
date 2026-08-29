use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use kmp_domain::{
    ContextPathNeighborhood, GraphNeighborhoodReader, MemoryAboutIndexReader, NodeDetailProjection,
    NodeDetailReader, NodeNeighborhood, NodeProjection, NodeRelationProjection,
    NodeRelationshipReader, NodeRelationships, PortError, ProjectionMutation, ProjectionWriter,
    RelationExplanation,
};
use tokio::sync::Mutex;

const PLACEHOLDER_TITLE: &str = "[unmaterialized node]";
const PLACEHOLDER_SUMMARY: &str = "Referenced by relation before node materialization";
const PLACEHOLDER_STATUS: &str = "UNMATERIALIZED";

/// Coherent in-memory kernel store: one shared graph + detail state where
/// `ProjectionWriter::apply_mutations` writes and every read port reads.
///
/// Unlike the fixture-style `InMemory*Reader` stores (canned lookups seeded
/// via `with_*`), this store implements the same observable storage semantics
/// as the infrastructure adapters (Neo4j graph + Valkey detail):
///
/// - `EnsureNode` creates only if absent; `UpsertNode` overwrites;
/// - `UpsertNodeRelation` materializes placeholder endpoints exactly like the
///   Neo4j `MERGE ... ON CREATE` path and upserts the edge keyed by
///   `(source, target, relation_type)`;
/// - `UpsertNodeDetail` is last-write-wins, mirroring the Valkey detail `SET`;
/// - `load_neighborhood` is a depth-bounded directed breadth-first traversal
///   over outgoing relations, returning all relations among selected nodes;
/// - `load_context_path` resolves a directed shortest path plus the target
///   subtree at `subtree_depth`;
/// - the about index lists `memory_anchor` nodes, optionally filtered through
///   outgoing `has_dimension` relations to `memory_dimension` nodes.
///
/// It is the reference implementation the conformance suite runs against and
/// the behavioral template for embedded adapters.
#[derive(Debug, Default, Clone)]
pub struct InMemoryKernelStore {
    state: Arc<Mutex<KernelStoreState>>,
}

#[derive(Debug, Default)]
struct KernelStoreState {
    nodes: BTreeMap<String, NodeProjection>,
    relations: BTreeMap<(String, String, String), RelationExplanation>,
    details: BTreeMap<String, NodeDetailProjection>,
}

impl KernelStoreState {
    fn ensure_node(&mut self, node: NodeProjection) {
        self.nodes.entry(node.node_id.clone()).or_insert(node);
    }

    fn upsert_node(&mut self, node: NodeProjection) {
        self.nodes.insert(node.node_id.clone(), node);
    }

    fn upsert_relation(&mut self, relation: NodeRelationProjection) {
        self.ensure_node(placeholder_node(&relation.source_node_id));
        self.ensure_node(placeholder_node(&relation.target_node_id));
        self.relations.insert(
            (
                relation.source_node_id,
                relation.target_node_id,
                relation.relation_type,
            ),
            relation.explanation,
        );
    }

    fn relations_among(&self, selected: &BTreeSet<String>) -> Vec<NodeRelationProjection> {
        self.relations
            .iter()
            .filter(|((source, target, _), _)| {
                selected.contains(source.as_str()) && selected.contains(target.as_str())
            })
            .map(
                |((source, target, relation_type), explanation)| NodeRelationProjection {
                    source_node_id: source.clone(),
                    target_node_id: target.clone(),
                    relation_type: relation_type.clone(),
                    explanation: explanation.clone(),
                },
            )
            .collect()
    }

    fn outgoing_targets<'a>(&'a self, node_id: &str) -> impl Iterator<Item = &'a str> + 'a {
        self.relations
            .range(
                (node_id.to_string(), String::new(), String::new())
                    ..(format!("{node_id}\u{0}"), String::new(), String::new()),
            )
            .map(|((_, target, _), _)| target.as_str())
    }

    fn reachable_outward(&self, root_node_id: &str, depth: u32) -> BTreeSet<String> {
        let mut visited = BTreeSet::from([root_node_id.to_string()]);
        let mut reachable = BTreeSet::new();
        let mut frontier = VecDeque::from([(root_node_id.to_string(), 0u32)]);

        while let Some((node_id, hops)) = frontier.pop_front() {
            if hops == depth {
                continue;
            }
            for target in self.outgoing_targets(&node_id) {
                if visited.insert(target.to_string()) {
                    reachable.insert(target.to_string());
                    frontier.push_back((target.to_string(), hops + 1));
                }
            }
        }

        reachable.remove(root_node_id);
        reachable
    }

    fn shortest_outward_path(
        &self,
        root_node_id: &str,
        target_node_id: &str,
    ) -> Option<Vec<String>> {
        let mut predecessors = BTreeMap::<String, String>::new();
        let mut visited = BTreeSet::from([root_node_id.to_string()]);
        let mut frontier = VecDeque::from([root_node_id.to_string()]);

        while let Some(node_id) = frontier.pop_front() {
            for target in self.outgoing_targets(&node_id) {
                if !visited.insert(target.to_string()) {
                    continue;
                }
                predecessors.insert(target.to_string(), node_id.clone());
                if target == target_node_id {
                    let mut path = vec![target.to_string()];
                    let mut current = target;
                    while let Some(previous) = predecessors.get(current) {
                        path.push(previous.clone());
                        current = previous;
                    }
                    path.reverse();
                    return Some(path);
                }
                frontier.push_back(target.to_string());
            }
        }

        None
    }

    fn selected_projections(
        &self,
        selected: &BTreeSet<String>,
        root_node_id: &str,
    ) -> Vec<NodeProjection> {
        selected
            .iter()
            .filter(|node_id| node_id.as_str() != root_node_id)
            .filter_map(|node_id| self.nodes.get(node_id).cloned())
            .collect()
    }
}

fn placeholder_node(node_id: &str) -> NodeProjection {
    NodeProjection {
        node_id: node_id.to_string(),
        node_kind: "placeholder".to_string(),
        title: PLACEHOLDER_TITLE.to_string(),
        summary: PLACEHOLDER_SUMMARY.to_string(),
        status: PLACEHOLDER_STATUS.to_string(),
        labels: vec!["placeholder".to_string()],
        properties: BTreeMap::from([
            ("placeholder".to_string(), "true".to_string()),
            (
                "placeholder_reason".to_string(),
                "relation_materialized_before_node".to_string(),
            ),
            (
                "placeholder_created_by_subject".to_string(),
                "graph.relation.materialized".to_string(),
            ),
        ]),
        provenance: None,
    }
}

impl InMemoryKernelStore {
    pub async fn node(&self, node_id: &str) -> Option<NodeProjection> {
        self.state.lock().await.nodes.get(node_id).cloned()
    }
}

impl ProjectionWriter for InMemoryKernelStore {
    async fn apply_mutations(&self, mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        let mut state = self.state.lock().await;
        for mutation in mutations {
            match mutation {
                ProjectionMutation::EnsureNode(node) => state.ensure_node(node),
                ProjectionMutation::UpsertNode(node) => state.upsert_node(node),
                ProjectionMutation::UpdateNodeStatus { node_id, status } => {
                    let node = state.nodes.get_mut(&node_id).ok_or_else(|| {
                        PortError::InvalidState(format!(
                            "cannot update missing in-memory node `{node_id}`"
                        ))
                    })?;
                    node.status = status;
                }
                ProjectionMutation::UpsertNodeRelation(relation) => {
                    state.upsert_relation(*relation);
                }
                ProjectionMutation::UpsertNodeDetail(detail) => {
                    state.details.insert(detail.node_id.clone(), detail);
                }
            }
        }
        Ok(())
    }
}

impl GraphNeighborhoodReader for InMemoryKernelStore {
    async fn load_neighborhood(
        &self,
        root_node_id: &str,
        depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        let state = self.state.lock().await;
        let Some(root) = state.nodes.get(root_node_id).cloned() else {
            return Ok(None);
        };

        let reachable = state.reachable_outward(root_node_id, depth);
        // The Neo4j neighborhood query only emits relation rows alongside
        // neighbor rows, so an empty neighborhood reports no relations even
        // when the root has a self-referential edge.
        let relations = if reachable.is_empty() {
            Vec::new()
        } else {
            let mut selected = reachable.clone();
            selected.insert(root_node_id.to_string());
            state.relations_among(&selected)
        };

        Ok(Some(NodeNeighborhood {
            neighbors: state.selected_projections(&reachable, root_node_id),
            relations,
            root,
        }))
    }

    async fn load_context_path(
        &self,
        root_node_id: &str,
        target_node_id: &str,
        subtree_depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        let state = self.state.lock().await;
        let Some(root) = state.nodes.get(root_node_id).cloned() else {
            return Ok(None);
        };
        if !state.nodes.contains_key(target_node_id) {
            return Ok(None);
        }
        let Some(path_node_ids) = state.shortest_outward_path(root_node_id, target_node_id) else {
            return Ok(None);
        };

        let mut selected = path_node_ids.iter().cloned().collect::<BTreeSet<_>>();
        selected.insert(target_node_id.to_string());
        selected.extend(state.reachable_outward(target_node_id, subtree_depth));

        Ok(Some(ContextPathNeighborhood {
            neighbors: state.selected_projections(&selected, root_node_id),
            relations: state.relations_among(&selected),
            path_node_ids,
            root,
        }))
    }
}

impl NodeRelationshipReader for InMemoryKernelStore {
    async fn load_node_relationships(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        let state = self.state.lock().await;
        if !state.nodes.contains_key(node_id) {
            return Ok(None);
        }

        let mut relationships = NodeRelationships::default();
        for ((source, target, relation_type), explanation) in &state.relations {
            let relation = NodeRelationProjection {
                source_node_id: source.clone(),
                target_node_id: target.clone(),
                relation_type: relation_type.clone(),
                explanation: explanation.clone(),
            };
            if target == node_id {
                relationships.incoming.push(relation.clone());
            }
            if source == node_id {
                relationships.outgoing.push(relation);
            }
        }
        Ok(Some(relationships))
    }
}

impl MemoryAboutIndexReader for InMemoryKernelStore {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        let state = self.state.lock().await;
        Ok(state
            .nodes
            .values()
            .filter(|node| node.node_kind == "memory_anchor")
            .map(|node| node.node_id.clone())
            .collect())
    }

    async fn list_memory_abouts_by_dimensions(
        &self,
        dimension_ids: &[String],
    ) -> Result<Vec<String>, PortError> {
        let state = self.state.lock().await;
        let mut abouts = BTreeSet::new();
        for (source, target, relation_type) in state.relations.keys() {
            if relation_type != "has_dimension" {
                continue;
            }
            let anchor_matches = state
                .nodes
                .get(source)
                .is_some_and(|node| node.node_kind == "memory_anchor");
            let dimension_matches = state.nodes.get(target).is_some_and(|node| {
                node.node_kind == "memory_dimension"
                    && dimension_ids.iter().any(|dimension_id| {
                        &node.node_id == dimension_id
                            || node
                                .node_id
                                .ends_with(&format!(":dimension:{dimension_id}"))
                    })
            });
            if anchor_matches && dimension_matches {
                abouts.insert(source.clone());
            }
        }
        Ok(abouts.into_iter().collect())
    }
}

impl NodeDetailReader for InMemoryKernelStore {
    async fn load_node_detail(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeDetailProjection>, PortError> {
        Ok(self.state.lock().await.details.get(node_id).cloned())
    }

    async fn load_node_details_batch(
        &self,
        node_ids: Vec<String>,
    ) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        let state = self.state.lock().await;
        Ok(node_ids
            .iter()
            .map(|node_id| state.details.get(node_id).cloned())
            .collect())
    }
}
