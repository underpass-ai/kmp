use kmp_ports::{NodeProjection, PortError};
use neo4rs::{Query, query};

use super::super::row_mapping::serialize_properties;

pub(crate) fn upsert_node_projection_query(node: &NodeProjection) -> Result<Query, PortError> {
    let mut q = query(
        "
MERGE (node:ProjectionNode {node_id: $node_id})
SET node.node_kind = $node_kind,
    node.title = $title,
    node.summary = $summary,
    node.status = $status,
    node.node_labels = $node_labels,
    node.properties_json = $properties_json,
    node.source_kind = $source_kind,
    node.source_agent = $source_agent,
    node.observed_at = $observed_at
        ",
    )
    .param("node_id", node.node_id.as_str())
    .param("node_kind", node.node_kind.as_str())
    .param("title", node.title.as_str())
    .param("summary", node.summary.as_str())
    .param("status", node.status.as_str())
    .param("node_labels", node.labels.clone())
    .param("properties_json", serialize_properties(&node.properties)?);

    if let Some(ref provenance) = node.provenance {
        q = q
            .param("source_kind", provenance.source_kind().as_str())
            .param(
                "source_agent",
                provenance.source_agent().unwrap_or_default(),
            )
            .param("observed_at", provenance.observed_at().unwrap_or_default());
    } else {
        q = q
            .param("source_kind", "")
            .param("source_agent", "")
            .param("observed_at", "");
    }

    Ok(q)
}

pub(crate) fn ensure_node_projection_query(node: &NodeProjection) -> Result<Query, PortError> {
    let mut q = query(
        "
MERGE (node:ProjectionNode {node_id: $node_id})
ON CREATE SET node.node_kind = $node_kind,
              node.title = $title,
              node.summary = $summary,
              node.status = $status,
              node.node_labels = $node_labels,
              node.properties_json = $properties_json,
              node.source_kind = $source_kind,
              node.source_agent = $source_agent,
              node.observed_at = $observed_at
        ",
    )
    .param("node_id", node.node_id.as_str())
    .param("node_kind", node.node_kind.as_str())
    .param("title", node.title.as_str())
    .param("summary", node.summary.as_str())
    .param("status", node.status.as_str())
    .param("node_labels", node.labels.clone())
    .param("properties_json", serialize_properties(&node.properties)?);

    if let Some(ref provenance) = node.provenance {
        q = q
            .param("source_kind", provenance.source_kind().as_str())
            .param(
                "source_agent",
                provenance.source_agent().unwrap_or_default(),
            )
            .param("observed_at", provenance.observed_at().unwrap_or_default());
    } else {
        q = q
            .param("source_kind", "")
            .param("source_agent", "")
            .param("observed_at", "");
    }

    Ok(q)
}
