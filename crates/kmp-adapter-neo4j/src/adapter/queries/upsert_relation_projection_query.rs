use kmp_ports::{NodeRelationProjection, PortError};
use neo4rs::{Query, query};

use super::super::row_mapping::serialize_properties;

const PLACEHOLDER_PROPERTIES_JSON: &str = "{\"placeholder\":\"true\",\"placeholder_reason\":\"relation_materialized_before_node\",\"placeholder_created_by_subject\":\"graph.relation.materialized\"}";

pub(crate) fn upsert_relation_projection_query(
    relation: &NodeRelationProjection,
) -> Result<Query, PortError> {
    Ok(query(
        "
MERGE (source:ProjectionNode {node_id: $source_node_id})
ON CREATE SET source.node_kind = 'placeholder',
              source.title = '[unmaterialized node]',
              source.summary = 'Referenced by relation before node materialization',
              source.status = 'UNMATERIALIZED',
              source.node_labels = ['placeholder'],
              source.properties_json = $placeholder_properties_json
MERGE (target:ProjectionNode {node_id: $target_node_id})
ON CREATE SET target.node_kind = 'placeholder',
              target.title = '[unmaterialized node]',
              target.summary = 'Referenced by relation before node materialization',
              target.status = 'UNMATERIALIZED',
              target.node_labels = ['placeholder'],
              target.properties_json = $placeholder_properties_json
MERGE (source)-[edge:RELATED_TO {relation_type: $relation_type}]->(target)
SET edge.properties_json = $properties_json
        ",
    )
    .param("source_node_id", relation.source_node_id.as_str())
    .param("target_node_id", relation.target_node_id.as_str())
    .param("relation_type", relation.relation_type.as_str())
    .param("placeholder_properties_json", PLACEHOLDER_PROPERTIES_JSON)
    .param(
        "properties_json",
        serialize_properties(&relation.explanation.to_properties())?,
    ))
}
