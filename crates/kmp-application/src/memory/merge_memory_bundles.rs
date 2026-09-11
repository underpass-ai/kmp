//! Ordered union shared by rendered context and structured temporal reads.
use crate::ApplicationError;
use kmp_domain::{BundleNode, BundleRelationship, KmpBundle};
use std::collections::BTreeSet;

pub(super) fn merge(mut bundles: Vec<KmpBundle>) -> Result<KmpBundle, ApplicationError> {
    if bundles.is_empty() {
        return Err(ApplicationError::Validation(
            "no memory context roots".into(),
        ));
    }
    let bundle = bundles.remove(0);
    if bundles.is_empty() {
        return Ok(bundle);
    }
    let mut node_ids = BTreeSet::from([bundle.root_node().node_id().to_string()]);
    let mut neighbor_nodes = bundle.neighbor_nodes().to_vec();
    for node in &neighbor_nodes {
        node_ids.insert(node.node_id().to_string());
    }

    let mut relationships = bundle.relationships().to_vec();
    let mut relationship_ids = relationships
        .iter()
        .map(relationship_key)
        .collect::<BTreeSet<_>>();
    let mut node_details = bundle.node_details().to_vec();
    let mut detail_ids = node_details
        .iter()
        .map(|detail| detail.node_id().to_string())
        .collect::<BTreeSet<_>>();

    for other in bundles {
        push_node(&mut neighbor_nodes, &mut node_ids, other.root_node());
        for node in other.neighbor_nodes() {
            push_node(&mut neighbor_nodes, &mut node_ids, node);
        }
        for relationship in other.relationships() {
            if relationship_ids.insert(relationship_key(relationship)) {
                relationships.push(relationship.clone());
            }
        }
        for detail in other.node_details() {
            if detail_ids.insert(detail.node_id().to_string()) {
                node_details.push(detail.clone());
            }
        }
    }

    KmpBundle::new(
        bundle.root_node_id().clone(),
        bundle.role().clone(),
        bundle.root_node().clone(),
        neighbor_nodes,
        relationships,
        node_details,
        bundle.metadata().clone(),
    )
    .map_err(ApplicationError::Domain)
}

fn push_node(
    neighbor_nodes: &mut Vec<BundleNode>,
    node_ids: &mut BTreeSet<String>,
    node: &BundleNode,
) {
    if node_ids.insert(node.node_id().to_string()) {
        neighbor_nodes.push(node.clone());
    }
}

fn relationship_key(relationship: &BundleRelationship) -> (String, String, String) {
    (
        relationship.source_node_id().to_string(),
        relationship.target_node_id().to_string(),
        relationship.relationship_type().to_string(),
    )
}
