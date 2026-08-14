use neo4rs::{Query, query};

pub(crate) fn load_incoming_node_relationships_query(node_id: &str) -> Query {
    query(
        "
MATCH (:ProjectionNode)-[edge:RELATED_TO]->(:ProjectionNode {node_id: $node_id})
RETURN startNode(edge).node_id AS source_node_id,
       endNode(edge).node_id AS target_node_id,
       edge.relation_type AS relation_type,
       coalesce(edge.properties_json, '{}') AS relation_properties_json
ORDER BY source_node_id, target_node_id, relation_type
        ",
    )
    .param("node_id", node_id)
}

pub(crate) fn load_outgoing_node_relationships_query(node_id: &str) -> Query {
    query(
        "
MATCH (:ProjectionNode {node_id: $node_id})-[edge:RELATED_TO]->(:ProjectionNode)
RETURN startNode(edge).node_id AS source_node_id,
       endNode(edge).node_id AS target_node_id,
       edge.relation_type AS relation_type,
       coalesce(edge.properties_json, '{}') AS relation_properties_json
ORDER BY source_node_id, target_node_id, relation_type
        ",
    )
    .param("node_id", node_id)
}
