use std::sync::Arc;

use kmp_domain::{NodeRelationProjection, NodeRelationshipReader};

use crate::ApplicationError;
use crate::queries::{GraphRelationshipView, QueryApplicationService};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetNodeRelationshipsQuery {
    pub node_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetNodeRelationshipsResult {
    pub incoming: Vec<GraphRelationshipView>,
    pub outgoing: Vec<GraphRelationshipView>,
    pub observed_at: std::time::SystemTime,
}

#[derive(Debug)]
pub struct GetNodeRelationshipsUseCase<G> {
    relationship_reader: G,
}

impl<G> GetNodeRelationshipsUseCase<G>
where
    G: NodeRelationshipReader + Send + Sync,
{
    pub fn new(relationship_reader: G) -> Self {
        Self {
            relationship_reader,
        }
    }

    pub async fn execute(
        &self,
        query: GetNodeRelationshipsQuery,
    ) -> Result<GetNodeRelationshipsResult, ApplicationError> {
        let node_id = trim_to_option(&query.node_id)
            .ok_or_else(|| ApplicationError::Validation("node_id cannot be empty".to_string()))?;
        let relationships = self
            .relationship_reader
            .load_node_relationships(&node_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("Node not found: {node_id}")))?;

        Ok(GetNodeRelationshipsResult {
            incoming: relationships
                .incoming
                .iter()
                .map(map_relationship)
                .collect(),
            outgoing: relationships
                .outgoing
                .iter()
                .map(map_relationship)
                .collect(),
            observed_at: std::time::SystemTime::now(),
        })
    }
}

impl<G, D, S> QueryApplicationService<G, D, S>
where
    G: NodeRelationshipReader + Send + Sync,
{
    pub async fn get_node_relationships(
        &self,
        query: GetNodeRelationshipsQuery,
    ) -> Result<GetNodeRelationshipsResult, ApplicationError> {
        GetNodeRelationshipsUseCase::new(Arc::clone(&self.graph_reader))
            .execute(query)
            .await
    }
}

fn map_relationship(relation: &NodeRelationProjection) -> GraphRelationshipView {
    GraphRelationshipView {
        source_node_id: relation.source_node_id.clone(),
        target_node_id: relation.target_node_id.clone(),
        relationship_type: relation.relation_type.clone(),
        explanation: relation.explanation.clone(),
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
    use kmp_domain::{
        NodeRelationProjection, NodeRelationshipReader, NodeRelationships, PortError,
        RelationExplanation, RelationSemanticClass,
    };

    use super::{GetNodeRelationshipsQuery, GetNodeRelationshipsUseCase};
    use crate::ApplicationError;

    struct MissingRelationshipReader;

    impl NodeRelationshipReader for MissingRelationshipReader {
        async fn load_node_relationships(
            &self,
            _node_id: &str,
        ) -> Result<Option<NodeRelationships>, PortError> {
            Ok(None)
        }
    }

    struct SeededRelationshipReader;

    impl NodeRelationshipReader for SeededRelationshipReader {
        async fn load_node_relationships(
            &self,
            node_id: &str,
        ) -> Result<Option<NodeRelationships>, PortError> {
            Ok(Some(NodeRelationships {
                incoming: vec![NodeRelationProjection {
                    source_node_id: "source-1".to_string(),
                    target_node_id: node_id.to_string(),
                    relation_type: "supports".to_string(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                        .with_rationale("source supports inspected node"),
                }],
                outgoing: vec![NodeRelationProjection {
                    source_node_id: node_id.to_string(),
                    target_node_id: "target-1".to_string(),
                    relation_type: "depends_on".to_string(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Constraint),
                }],
            }))
        }
    }

    #[tokio::test]
    async fn execute_returns_direct_incoming_and_outgoing_links() {
        let result = GetNodeRelationshipsUseCase::new(SeededRelationshipReader)
            .execute(GetNodeRelationshipsQuery {
                node_id: "node-123".to_string(),
            })
            .await
            .expect("relationships should load");

        assert_eq!(result.incoming.len(), 1);
        assert_eq!(result.incoming[0].target_node_id, "node-123");
        assert_eq!(result.outgoing.len(), 1);
        assert_eq!(result.outgoing[0].source_node_id, "node-123");
    }

    #[tokio::test]
    async fn execute_fails_for_missing_node() {
        let error = GetNodeRelationshipsUseCase::new(MissingRelationshipReader)
            .execute(GetNodeRelationshipsQuery {
                node_id: "missing".to_string(),
            })
            .await
            .expect_err("missing node should fail");

        match error {
            ApplicationError::NotFound(message) => assert_eq!(message, "Node not found: missing"),
            other => panic!("unexpected error: {other}"),
        }
    }
}
