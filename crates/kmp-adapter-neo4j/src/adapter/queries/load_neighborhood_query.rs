use neo4rs::Query;

pub(crate) fn load_neighborhood_query(root_node_id: &str, depth: u32) -> Query {
    Query::new(format!(
        "
MATCH (root:ProjectionNode {{node_id: $root_node_id}})
OPTIONAL MATCH path = (root)-[:RELATED_TO*1..{depth}]->(reachable:ProjectionNode)
WITH root, [node IN collect(DISTINCT reachable) WHERE node IS NOT NULL AND node <> root] AS reachable_nodes
WITH root, reachable_nodes, reachable_nodes + [root] AS selected_nodes
UNWIND CASE WHEN size(reachable_nodes) = 0 THEN [NULL] ELSE reachable_nodes END AS neighbor
OPTIONAL MATCH (source:ProjectionNode)-[edge:RELATED_TO]->(target:ProjectionNode)
WHERE neighbor IS NOT NULL
  AND source IN selected_nodes
  AND target IN selected_nodes
RETURN coalesce(neighbor.node_id, '') AS neighbor_node_id,
       coalesce(neighbor.node_kind, '') AS neighbor_node_kind,
       coalesce(neighbor.title, '') AS neighbor_title,
       coalesce(neighbor.summary, '') AS neighbor_summary,
       coalesce(neighbor.status, '') AS neighbor_status,
       coalesce(neighbor.node_labels, []) AS neighbor_node_labels,
       coalesce(neighbor.properties_json, '{{}}') AS neighbor_properties_json,
       coalesce(neighbor.source_kind, '') AS neighbor_source_kind,
       coalesce(neighbor.source_agent, '') AS neighbor_source_agent,
       coalesce(neighbor.observed_at, '') AS neighbor_observed_at,
       CASE WHEN edge IS NULL THEN '' ELSE startNode(edge).node_id END AS source_node_id,
       CASE WHEN edge IS NULL THEN '' ELSE endNode(edge).node_id END AS target_node_id,
       coalesce(edge.relation_type, '') AS relation_type,
       coalesce(edge.properties_json, '{{}}') AS relation_properties_json
        ",
    ))
    .param("root_node_id", root_node_id)
}
