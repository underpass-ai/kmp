use kmp_domain::{BundleNode, BundleNodeDetail, BundleRelationship, KmpBundle};
use kmp_ports::PortError;
use serde_json::{Value, json};

pub(crate) fn serialize_bundle(bundle: &KmpBundle) -> Result<String, PortError> {
    serde_json::to_string(&json!({
        "root_node_id": bundle.root_node_id().as_str(),
        "role": bundle.role().as_str(),
        "root_node": serialize_node(bundle.root_node()),
        "neighbor_nodes": bundle
            .neighbor_nodes()
            .iter()
            .map(serialize_node)
            .collect::<Vec<_>>(),
        "relationships": bundle
            .relationships()
            .iter()
            .map(serialize_relationship)
            .collect::<Vec<_>>(),
        "node_details": bundle
            .node_details()
            .iter()
            .map(serialize_detail)
            .collect::<Vec<_>>(),
        "stats": {
            "selected_nodes": bundle.stats().selected_nodes(),
            "selected_relationships": bundle.stats().selected_relationships(),
            "detailed_nodes": bundle.stats().detailed_nodes(),
        },
        "metadata": {
            "revision": bundle.metadata().revision,
            "content_hash": bundle.metadata().content_hash,
            "generator_version": bundle.metadata().generator_version,
        }
    }))
    .map_err(|error| {
        PortError::InvalidState(format!(
            "bundle could not be serialized for valkey: {error}"
        ))
    })
}

fn serialize_node(node: &BundleNode) -> Value {
    json!({
        "node_id": node.node_id(),
        "node_kind": node.node_kind(),
        "title": node.title(),
        "summary": node.summary(),
        "status": node.status(),
        "labels": node.labels(),
        "properties": node.properties(),
    })
}

fn serialize_relationship(relationship: &BundleRelationship) -> Value {
    json!({
        "source_node_id": relationship.source_node_id(),
        "target_node_id": relationship.target_node_id(),
        "relationship_type": relationship.relationship_type(),
        "explanation": relationship.explanation().to_properties(),
    })
}

fn serialize_detail(detail: &BundleNodeDetail) -> Value {
    json!({
        "node_id": detail.node_id(),
        "detail": detail.detail(),
        "content_hash": detail.content_hash(),
        "revision": detail.revision(),
    })
}
