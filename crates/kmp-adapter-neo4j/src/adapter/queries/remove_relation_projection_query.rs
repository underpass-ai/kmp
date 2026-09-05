use neo4rs::{Query, query};

/// Drops one projected relation by its identity. Removing an absent
/// relation matches nothing and is not an error; the nodes at either end
/// stay, as the embedded store leaves them.
pub(crate) fn remove_relation_projection_query(
    source_node_id: &str,
    target_node_id: &str,
    relation_type: &str,
) -> Query {
    query(
        "
MATCH (source:ProjectionNode {node_id: $source_node_id})
      -[edge:RELATED_TO {relation_type: $relation_type}]->
      (target:ProjectionNode {node_id: $target_node_id})
DELETE edge
        ",
    )
    .param("source_node_id", source_node_id)
    .param("target_node_id", target_node_id)
    .param("relation_type", relation_type)
}
