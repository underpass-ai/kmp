use neo4rs::{Query, query};

pub(crate) fn load_root_node_query(root_node_id: &str) -> Query {
    query(
        "
MATCH (root:ProjectionNode {node_id: $root_node_id})
RETURN root.node_id AS node_id,
       coalesce(root.node_kind, '') AS node_kind,
       coalesce(root.title, '') AS title,
       coalesce(root.summary, '') AS summary,
       coalesce(root.status, '') AS status,
       coalesce(root.node_labels, []) AS node_labels,
       coalesce(root.properties_json, '{}') AS properties_json,
       coalesce(root.source_kind, '') AS source_kind,
       coalesce(root.source_agent, '') AS source_agent,
       coalesce(root.observed_at, '') AS observed_at
LIMIT 1
        ",
    )
    .param("root_node_id", root_node_id)
}
