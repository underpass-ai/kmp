use std::collections::BTreeSet;

use kmp_domain::RelationExplanation;
use kmp_ports::{NodeRelationProjection, NodeRelationshipReader, NodeRelationships, PortError};
use neo4rs::Graph;

use super::projection_store::Neo4jProjectionStore;
use super::queries::{
    load_incoming_node_relationships_query, load_outgoing_node_relationships_query,
    load_root_node_query,
};
use super::row_mapping::{row_properties, row_string};

impl Neo4jProjectionStore {
    async fn load_incoming_relationship_rows(
        &self,
        graph: &Graph,
        node_id: &str,
    ) -> Result<Vec<neo4rs::Row>, PortError> {
        self.fetch_rows(
            graph,
            load_incoming_node_relationships_query(node_id),
            &format!("load incoming node relationships for `{node_id}`"),
        )
        .await
    }

    async fn load_outgoing_relationship_rows(
        &self,
        graph: &Graph,
        node_id: &str,
    ) -> Result<Vec<neo4rs::Row>, PortError> {
        self.fetch_rows(
            graph,
            load_outgoing_node_relationships_query(node_id),
            &format!("load outgoing node relationships for `{node_id}`"),
        )
        .await
    }
}

impl NodeRelationshipReader for Neo4jProjectionStore {
    async fn load_node_relationships(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        let graph = self.graph().await?;
        let exists = self
            .fetch_optional_row(
                &graph,
                load_root_node_query(node_id),
                &format!("load relationship root node for `{node_id}`"),
            )
            .await?
            .is_some();
        if !exists {
            return Ok(None);
        }

        let incoming_rows = self
            .load_incoming_relationship_rows(&graph, node_id)
            .await?;
        let outgoing_rows = self
            .load_outgoing_relationship_rows(&graph, node_id)
            .await?;

        Ok(Some(NodeRelationships {
            incoming: map_relationship_rows(incoming_rows, "incoming node relationship")?,
            outgoing: map_relationship_rows(outgoing_rows, "outgoing node relationship")?,
        }))
    }
}

fn map_relationship_rows(
    rows: Vec<neo4rs::Row>,
    entity: &str,
) -> Result<Vec<NodeRelationProjection>, PortError> {
    let mut relation_keys = BTreeSet::<(String, String, String)>::new();
    let mut relationships = Vec::new();

    for row in rows {
        let source_node_id = row_string(&row, "source_node_id", entity)?;
        let target_node_id = row_string(&row, "target_node_id", entity)?;
        let relation_type = row_string(&row, "relation_type", entity)?;
        let explanation = RelationExplanation::from_properties(&row_properties(
            &row,
            "relation_properties_json",
            entity,
        )?)
        .map_err(|error| {
            PortError::InvalidState(format!(
                "neo4j {entity} explanation could not be decoded: {error}"
            ))
        })?;
        let relation_key = (
            source_node_id.clone(),
            target_node_id.clone(),
            relation_type.clone(),
        );

        if relation_keys.insert(relation_key) {
            relationships.push(NodeRelationProjection {
                source_node_id,
                target_node_id,
                relation_type,
                explanation,
            });
        }
    }

    Ok(relationships)
}
