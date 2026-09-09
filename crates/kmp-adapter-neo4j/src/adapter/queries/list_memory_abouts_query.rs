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
/// bare value, or by kind, which the embedded index
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
  AND (dimension.node_id IN $dimension_ids
    OR any(fragment IN $label_fragments
      WHERE dimension.properties_json CONTAINS fragment))
RETURN DISTINCT anchor.node_id AS about
ORDER BY about
        ",
    )
    .param("dimension_ids", dimension_ids.to_vec())
    .param("label_fragments", label_fragments(dimension_ids))
}

fn label_fragments(values: &[String]) -> Vec<String> {
    values
        .iter()
        .flat_map(|value| {
            // Match the exact serialized JSON value, including escaped quotes,
            // backslashes and Unicode. Query parameters never become Cypher.
            let value = serde_json::to_string(value).expect("a string serializes");
            [
                format!("\"dimension_value\":{value}"),
                format!("\"dimension_kind\":{value}"),
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_filter_matches_punctuation_without_matching_longer_values() {
        let value = "Nébula \\\"west\\\"";
        let properties = serde_json::json!({"dimension_value": value}).to_string();
        let fragments = label_fragments(&[value.to_string()]);
        assert!(properties.contains(&fragments[0]));
        let longer = serde_json::json!({"dimension_value": format!("{value} extra")}).to_string();
        assert!(!longer.contains(&fragments[0]));
    }
}
