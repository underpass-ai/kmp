use std::collections::BTreeMap;

use kmp_domain::{GraphNeighborhoodReader, NodeNeighborhood, RelationExplanation};

use crate::ApplicationError;
use crate::queries::clamp_native_graph_traversal_depth;
use crate::queries::ordered_neighborhood::ordered_neighborhood;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetGraphRelationshipsQuery {
    pub node_id: String,
    pub node_kind: Option<String>,
    pub depth: u32,
    pub include_reverse_edges: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNodeView {
    pub node_id: String,
    pub node_kind: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub labels: Vec<String>,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRelationshipView {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relationship_type: String,
    pub explanation: RelationExplanation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetGraphRelationshipsResult {
    pub root: GraphNodeView,
    pub neighbors: Vec<GraphNodeView>,
    pub relationships: Vec<GraphRelationshipView>,
    pub observed_at: std::time::SystemTime,
}

#[derive(Debug)]
pub struct GetGraphRelationshipsUseCase<G> {
    graph_reader: G,
}

impl<G> GetGraphRelationshipsUseCase<G>
where
    G: GraphNeighborhoodReader + Send + Sync,
{
    pub fn new(graph_reader: G) -> Self {
        Self { graph_reader }
    }

    pub async fn execute(
        &self,
        query: GetGraphRelationshipsQuery,
    ) -> Result<GetGraphRelationshipsResult, ApplicationError> {
        let node_id = trim_to_option(&query.node_id)
            .ok_or_else(|| ApplicationError::Validation("node_id cannot be empty".to_string()))?;
        let neighborhood = ordered_neighborhood(
            load_existing_neighborhood(
                &self.graph_reader,
                &node_id,
                clamp_native_graph_traversal_depth(query.depth),
            )
            .await?,
        );

        Ok(GetGraphRelationshipsResult {
            root: map_node(&neighborhood.root),
            neighbors: neighborhood.neighbors.iter().map(map_node).collect(),
            relationships: neighborhood
                .relations
                .iter()
                .map(|relation| GraphRelationshipView {
                    source_node_id: relation.source_node_id.clone(),
                    target_node_id: relation.target_node_id.clone(),
                    relationship_type: relation.relation_type.clone(),
                    explanation: relation.explanation.clone(),
                })
                .collect(),
            observed_at: std::time::SystemTime::now(),
        })
    }
}

async fn load_existing_neighborhood<G>(
    graph_reader: &G,
    node_id: &str,
    depth: u32,
) -> Result<NodeNeighborhood, ApplicationError>
where
    G: GraphNeighborhoodReader + Send + Sync,
{
    graph_reader
        .load_neighborhood(node_id, depth)
        .await?
        .ok_or_else(|| ApplicationError::Validation(format!("Node not found: {node_id}")))
}

fn map_node(node: &kmp_domain::NodeProjection) -> GraphNodeView {
    GraphNodeView {
        node_id: node.node_id.clone(),
        node_kind: node.node_kind.clone(),
        title: node.title.clone(),
        summary: node.summary.clone(),
        status: node.status.clone(),
        labels: node.labels.clone(),
        properties: node.properties.clone(),
    }
}

fn trim_to_option(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use kmp_domain::{
        ContextPathNeighborhood, NodeNeighborhood, NodeProjection, NodeRelationProjection,
        PortError, RelationExplanation, RelationSemanticClass,
    };
    use tokio::sync::Mutex;

    use super::{GetGraphRelationshipsQuery, GetGraphRelationshipsUseCase};
    use crate::ApplicationError;
    use crate::queries::MAX_NATIVE_GRAPH_TRAVERSAL_DEPTH;

    struct MissingGraphReader;

    impl kmp_domain::GraphNeighborhoodReader for MissingGraphReader {
        async fn load_neighborhood(
            &self,
            _root_node_id: &str,
            _depth: u32,
        ) -> Result<Option<NodeNeighborhood>, PortError> {
            Ok(None)
        }

        async fn load_context_path(
            &self,
            _root_node_id: &str,
            _target_node_id: &str,
            _subtree_depth: u32,
        ) -> Result<Option<ContextPathNeighborhood>, PortError> {
            Ok(None)
        }
    }

    struct SeededGraphReader;

    impl kmp_domain::GraphNeighborhoodReader for SeededGraphReader {
        async fn load_neighborhood(
            &self,
            root_node_id: &str,
            _depth: u32,
        ) -> Result<Option<NodeNeighborhood>, PortError> {
            Ok(Some(NodeNeighborhood {
                root: NodeProjection {
                    node_id: root_node_id.to_string(),
                    node_kind: "story".to_string(),
                    title: "Root".to_string(),
                    summary: "Root summary".to_string(),
                    status: "ACTIVE".to_string(),
                    labels: vec!["Story".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                },
                neighbors: vec![NodeProjection {
                    node_id: "neighbor-1".to_string(),
                    node_kind: "task".to_string(),
                    title: "Neighbor".to_string(),
                    summary: "Neighbor summary".to_string(),
                    status: "OPEN".to_string(),
                    labels: vec!["Task".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                }],
                relations: vec![NodeRelationProjection {
                    source_node_id: root_node_id.to_string(),
                    target_node_id: "neighbor-1".to_string(),
                    relation_type: "RELATES_TO".to_string(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Motivational)
                        .with_rationale("neighbor-1 is the next actionable item"),
                }],
            }))
        }

        async fn load_context_path(
            &self,
            _root_node_id: &str,
            _target_node_id: &str,
            _subtree_depth: u32,
        ) -> Result<Option<ContextPathNeighborhood>, PortError> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn execute_rejects_missing_node() {
        let use_case = GetGraphRelationshipsUseCase::new(MissingGraphReader);

        let error = use_case
            .execute(GetGraphRelationshipsQuery {
                node_id: "missing-123".to_string(),
                node_kind: Some("Story".to_string()),
                depth: 2,
                include_reverse_edges: false,
            })
            .await
            .expect_err("missing node should be rejected");

        match error {
            ApplicationError::Validation(message) => {
                assert_eq!(message, "Node not found: missing-123")
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[tokio::test]
    async fn execute_returns_graph_views_for_existing_node() {
        let use_case = GetGraphRelationshipsUseCase::new(SeededGraphReader);

        let result = use_case
            .execute(GetGraphRelationshipsQuery {
                node_id: "story-123".to_string(),
                node_kind: Some("Story".to_string()),
                depth: 2,
                include_reverse_edges: false,
            })
            .await
            .expect("existing node should succeed");

        assert_eq!(result.root.node_id, "story-123");
        assert_eq!(result.neighbors.len(), 1);
        assert_eq!(result.relationships.len(), 1);
        assert_eq!(result.relationships[0].target_node_id, "neighbor-1");
        assert_eq!(
            result.relationships[0].explanation.rationale(),
            Some("neighbor-1 is the next actionable item")
        );
    }

    struct RecordingGraphReader {
        depths: Arc<Mutex<Vec<u32>>>,
    }

    impl kmp_domain::GraphNeighborhoodReader for RecordingGraphReader {
        async fn load_neighborhood(
            &self,
            root_node_id: &str,
            depth: u32,
        ) -> Result<Option<NodeNeighborhood>, PortError> {
            self.depths.lock().await.push(depth);
            Ok(Some(NodeNeighborhood {
                root: NodeProjection {
                    node_id: root_node_id.to_string(),
                    node_kind: "story".to_string(),
                    title: "Root".to_string(),
                    summary: "Root summary".to_string(),
                    status: "ACTIVE".to_string(),
                    labels: vec!["Story".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                },
                neighbors: Vec::new(),
                relations: Vec::new(),
            }))
        }

        async fn load_context_path(
            &self,
            _root_node_id: &str,
            _target_node_id: &str,
            _subtree_depth: u32,
        ) -> Result<Option<ContextPathNeighborhood>, PortError> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn execute_clamps_and_forwards_depth_to_graph_reader() {
        let depths = Arc::new(Mutex::new(Vec::new()));
        let use_case = GetGraphRelationshipsUseCase::new(RecordingGraphReader {
            depths: Arc::clone(&depths),
        });

        use_case
            .execute(GetGraphRelationshipsQuery {
                node_id: "story-123".to_string(),
                node_kind: None,
                depth: 99,
                include_reverse_edges: false,
            })
            .await
            .expect("existing node should succeed");

        assert_eq!(&*depths.lock().await, &[MAX_NATIVE_GRAPH_TRAVERSAL_DEPTH]);
    }
}
