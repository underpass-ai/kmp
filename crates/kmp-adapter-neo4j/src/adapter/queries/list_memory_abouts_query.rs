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

/// The abouts whose dimension nodes match any of the ids: by node id, by
/// bare scope (`…:dimension:<id>`), or by kind, which the embedded index
/// reads from `dimension_kind` and this one from the serialized properties
/// (`serialize_properties` writes compact JSON, so the fragment is exact).
/// A selection names kinds (`include`, an `exists` selector) as readily as
/// values (`scope_ids`, an `in` selector); the filter that follows reads
/// both, so the index that picks the abouts must too.
pub(crate) fn list_memory_abouts_by_dimensions_query(dimension_ids: &[String]) -> Query {
    query(
        "
MATCH (anchor:ProjectionNode)-[edge:RELATED_TO]->(dimension:ProjectionNode)
WHERE anchor.node_kind = 'memory_anchor'
  AND edge.relation_type = 'has_dimension'
  AND dimension.node_kind = 'memory_dimension'
  AND any(dimension_id IN $dimension_ids
    WHERE dimension.node_id = dimension_id
       OR dimension.node_id ENDS WITH (':dimension:' + dimension_id)
       OR dimension.properties_json CONTAINS ('\"dimension_kind\":\"' + dimension_id + '\"'))
RETURN DISTINCT anchor.node_id AS about
ORDER BY about
        ",
    )
    .param("dimension_ids", dimension_ids.to_vec())
}
