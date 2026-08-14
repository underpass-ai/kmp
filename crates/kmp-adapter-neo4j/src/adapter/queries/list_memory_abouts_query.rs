use neo4rs::{Query, query};

pub(crate) fn list_memory_abouts_query() -> Query {
    Query::new(
        "
MATCH (anchor:ProjectionNode)
WHERE anchor.node_kind = 'memory_anchor'
RETURN anchor.node_id AS about
ORDER BY about
        "
        .to_string(),
    )
}

pub(crate) fn list_memory_abouts_by_dimensions_query(dimension_ids: &[String]) -> Query {
    query(
        "
MATCH (anchor:ProjectionNode)-[edge:RELATED_TO]->(dimension:ProjectionNode)
WHERE anchor.node_kind = 'memory_anchor'
  AND edge.relation_type = 'has_dimension'
  AND dimension.node_kind = 'memory_dimension'
  AND any(dimension_id IN $dimension_ids
    WHERE dimension.node_id = dimension_id
       OR dimension.node_id ENDS WITH (':dimension:' + dimension_id))
RETURN DISTINCT anchor.node_id AS about
ORDER BY about
        ",
    )
    .param("dimension_ids", dimension_ids.to_vec())
}
