use neo4rs::Query;

pub(crate) fn load_context_path_query(
    root_node_id: &str,
    target_node_id: &str,
    subtree_depth: u32,
) -> Query {
    Query::new(format!(
        "
MATCH (root:ProjectionNode {{node_id: $root_node_id}})
MATCH (target_node:ProjectionNode {{node_id: $target_node_id}})
MATCH path = shortestPath((root)-[:RELATED_TO*]->(target_node))
WITH root, target_node, nodes(path) AS path_nodes
OPTIONAL MATCH (target_node)-[:RELATED_TO*0..{subtree_depth}]->(descendant:ProjectionNode)
WITH root,
     path_nodes,
     [node IN collect(DISTINCT descendant) WHERE node IS NOT NULL] AS subtree_nodes
UNWIND path_nodes + subtree_nodes AS selected_node
WITH root, path_nodes, collect(DISTINCT selected_node) AS selected_nodes
UNWIND [node IN selected_nodes WHERE node <> root] AS neighbor
OPTIONAL MATCH (source:ProjectionNode)-[edge:RELATED_TO]->(rel_target:ProjectionNode)
WHERE source IN selected_nodes
  AND rel_target IN selected_nodes
RETURN [node IN path_nodes | node.node_id] AS path_node_ids,
       coalesce(neighbor.node_id, '') AS neighbor_node_id,
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
    .param("target_node_id", target_node_id)
}
