use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::RelationExplanation;
use kmp_ports::{
    ContextPathNeighborhood, GraphNeighborhoodReader, NodeNeighborhood, NodeProjection,
    NodeRelationProjection, PortError,
};
use neo4rs::Graph;

use super::projection_store::Neo4jProjectionStore;
use super::queries::{load_context_path_query, load_neighborhood_query, load_root_node_query};
use super::row_mapping::{node_projection_from_row, row_properties, row_string, row_string_vec};

impl Neo4jProjectionStore {
    async fn load_root_node(
        &self,
        graph: &Graph,
        root_node_id: &str,
    ) -> Result<Option<NodeProjection>, PortError> {
        let Some(row) = self
            .fetch_optional_row(
                graph,
                load_root_node_query(root_node_id),
                &format!("load root node for `{root_node_id}`"),
            )
            .await?
        else {
            return Ok(None);
        };

        Ok(Some(node_projection_from_row(&row, "", "root node")?))
    }

    async fn load_neighbor_rows(
        &self,
        graph: &Graph,
        root_node_id: &str,
        depth: u32,
    ) -> Result<Vec<neo4rs::Row>, PortError> {
        self.fetch_rows(
            graph,
            load_neighborhood_query(root_node_id, depth),
            &format!("load node neighborhood for `{root_node_id}`"),
        )
        .await
    }

    async fn load_context_path_rows(
        &self,
        graph: &Graph,
        root_node_id: &str,
        target_node_id: &str,
        subtree_depth: u32,
    ) -> Result<Vec<neo4rs::Row>, PortError> {
        self.fetch_rows(
            graph,
            load_context_path_query(root_node_id, target_node_id, subtree_depth),
            &format!("load context path from `{root_node_id}` to `{target_node_id}`"),
        )
        .await
    }
}

impl GraphNeighborhoodReader for Neo4jProjectionStore {
    async fn load_neighborhood(
        &self,
        root_node_id: &str,
        depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        let graph = self.graph().await?;
        let Some(root) = self.load_root_node(&graph, root_node_id).await? else {
            return Ok(None);
        };
        let rows = self.load_neighbor_rows(&graph, root_node_id, depth).await?;

        let mut neighbors_by_id = BTreeMap::<String, NodeProjection>::new();
        let mut relation_keys = BTreeSet::<(String, String, String)>::new();
        let mut relations = Vec::new();

        for row in rows {
            let neighbor_node_id = row_string(&row, "neighbor_node_id", "neighbor node")?;
            let relation_type = row_string(&row, "relation_type", "neighbor relation")?;
            if neighbor_node_id.is_empty() || relation_type.is_empty() {
                continue;
            }

            neighbors_by_id
                .entry(neighbor_node_id.clone())
                .or_insert(node_projection_from_row(
                    &row,
                    "neighbor_",
                    "neighbor node",
                )?);

            let source_node_id = row_string(&row, "source_node_id", "neighbor relation")?;
            let target_node_id = row_string(&row, "target_node_id", "neighbor relation")?;
            let explanation = RelationExplanation::from_properties(&row_properties(
                &row,
                "relation_properties_json",
                "neighbor relation",
            )?)
            .map_err(|error| {
                PortError::InvalidState(format!(
                    "neo4j neighbor relation explanation could not be decoded: {error}"
                ))
            })?;
            let relation_key = (
                source_node_id.clone(),
                target_node_id.clone(),
                relation_type.clone(),
            );

            if relation_keys.insert(relation_key) {
                relations.push(NodeRelationProjection {
                    source_node_id,
                    target_node_id,
                    relation_type,
                    explanation,
                });
            }
        }

        Ok(Some(NodeNeighborhood {
            root,
            neighbors: neighbors_by_id.into_values().collect(),
            relations,
        }))
    }

    async fn load_context_path(
        &self,
        root_node_id: &str,
        target_node_id: &str,
        subtree_depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        let graph = self.graph().await?;
        let Some(root) = self.load_root_node(&graph, root_node_id).await? else {
            return Ok(None);
        };
        let rows = self
            .load_context_path_rows(&graph, root_node_id, target_node_id, subtree_depth)
            .await?;

        if rows.is_empty() {
            return Ok(None);
        }

        let path_node_ids = row_string_vec(&rows[0], "path_node_ids", "context path")?;
        let mut neighbors_by_id = BTreeMap::<String, NodeProjection>::new();
        let mut relation_keys = BTreeSet::<(String, String, String)>::new();
        let mut relations = Vec::new();

        for row in rows {
            let neighbor_node_id = row_string(&row, "neighbor_node_id", "context path node")?;
            let relation_type = row_string(&row, "relation_type", "context path relation")?;
            if !neighbor_node_id.is_empty() {
                neighbors_by_id.entry(neighbor_node_id.clone()).or_insert(
                    node_projection_from_row(&row, "neighbor_", "context path node")?,
                );
            }

            if relation_type.is_empty() {
                continue;
            }

            let source_node_id = row_string(&row, "source_node_id", "context path relation")?;
            let target_node_id = row_string(&row, "target_node_id", "context path relation")?;
            let explanation = RelationExplanation::from_properties(&row_properties(
                &row,
                "relation_properties_json",
                "context path relation",
            )?)
            .map_err(|error| {
                PortError::InvalidState(format!(
                    "neo4j context path relation explanation could not be decoded: {error}"
                ))
            })?;
            let relation_key = (
                source_node_id.clone(),
                target_node_id.clone(),
                relation_type.clone(),
            );

            if relation_keys.insert(relation_key) {
                relations.push(NodeRelationProjection {
                    source_node_id,
                    target_node_id,
                    relation_type,
                    explanation,
                });
            }
        }

        Ok(Some(ContextPathNeighborhood {
            root,
            neighbors: neighbors_by_id.into_values().collect(),
            relations,
            path_node_ids,
        }))
    }
}
