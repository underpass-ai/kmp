use kmp_ports::{PortError, ProjectionMutation, ProjectionWriter};
use neo4rs::Graph;

use super::projection_store::Neo4jProjectionStore;
use super::queries::{
    ensure_node_projection_query, upsert_node_projection_query, upsert_relation_projection_query,
};

impl Neo4jProjectionStore {
    async fn apply_node_projection(
        &self,
        graph: &Graph,
        node: &kmp_ports::NodeProjection,
    ) -> Result<(), PortError> {
        self.run_query(
            graph,
            upsert_node_projection_query(node)?,
            &format!("apply node projection for `{}`", node.node_id),
        )
        .await
    }

    async fn ensure_node_projection(
        &self,
        graph: &Graph,
        node: &kmp_ports::NodeProjection,
    ) -> Result<(), PortError> {
        self.run_query(
            graph,
            ensure_node_projection_query(node)?,
            &format!("ensure node projection for `{}`", node.node_id),
        )
        .await
    }

    async fn apply_relation_projection(
        &self,
        graph: &Graph,
        relation: &kmp_ports::NodeRelationProjection,
    ) -> Result<(), PortError> {
        self.run_query(
            graph,
            upsert_relation_projection_query(relation)?,
            &format!(
                "apply relation projection for `{} -> {}`",
                relation.source_node_id, relation.target_node_id
            ),
        )
        .await
    }
}

impl ProjectionWriter for Neo4jProjectionStore {
    async fn apply_mutations(&self, mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        let graph = self.graph().await?;

        for mutation in mutations {
            match mutation {
                ProjectionMutation::EnsureNode(node) => {
                    self.ensure_node_projection(&graph, &node).await?;
                }
                ProjectionMutation::UpsertNode(node) => {
                    self.apply_node_projection(&graph, &node).await?;
                }
                ProjectionMutation::UpsertNodeRelation(relation) => {
                    self.apply_relation_projection(&graph, &relation).await?;
                }
                ProjectionMutation::UpsertNodeDetail(detail) => {
                    return Err(PortError::InvalidState(format!(
                        "neo4j graph projection writer does not persist node detail `{}`",
                        detail.node_id
                    )));
                }
            }
        }

        Ok(())
    }
}
