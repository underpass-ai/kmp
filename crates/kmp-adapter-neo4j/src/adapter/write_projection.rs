use kmp_ports::{PortError, ProjectionMutation, ProjectionWriter};
use neo4rs::Graph;

use super::projection_store::Neo4jProjectionStore;
use super::queries::{
    ensure_node_projection_query, remove_relation_projection_query, update_node_status_query,
    upsert_node_projection_query, upsert_relation_projection_query,
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
                ProjectionMutation::UpdateNodeStatus { node_id, status } => {
                    self.run_query(
                        &graph,
                        update_node_status_query(&node_id, &status),
                        &format!("update status for `{node_id}`"),
                    )
                    .await?;
                }
                ProjectionMutation::UpsertNodeRelation(relation) => {
                    self.apply_relation_projection(&graph, &relation).await?;
                }
                ProjectionMutation::RemoveNodeRelation {
                    source_node_id,
                    target_node_id,
                    relation_type,
                } => {
                    self.run_query(
                        &graph,
                        remove_relation_projection_query(
                            &source_node_id,
                            &target_node_id,
                            &relation_type,
                        ),
                        &format!(
                            "remove relation projection for `{source_node_id} -> {target_node_id}`"
                        ),
                    )
                    .await?;
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
