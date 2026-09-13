use crate::ApplicationError;
use kmp_domain::{BundleNode, BundleRelationship, KmpBundle};
use std::collections::BTreeSet;

/// A temporal index needs entry text, node kinds and membership coordinates.
/// Canonical source prose and payload metadata are materialized only for the
/// selected proof. No shortened source is used for ranking or returned as proof.
pub(super) fn admission_catalogue(bundle: KmpBundle) -> Result<KmpBundle, ApplicationError> {
    let root = header(bundle.root_node());
    let neighbors = bundle.neighbor_nodes().iter().map(header).collect();
    let sources = std::iter::once(bundle.root_node())
        .chain(bundle.neighbor_nodes())
        .filter(|node| matches!(node.node_kind(), "memory_evidence" | "evidence"))
        .map(|node| node.node_id())
        .collect::<BTreeSet<_>>();
    let relations = bundle
        .relationships()
        .iter()
        .map(|edge| {
            if edge.relationship_type() == "supports" && sources.contains(edge.source_node_id()) {
                let mut header = BundleRelationship::new(
                    edge.source_node_id(),
                    edge.target_node_id(),
                    edge.relationship_type(),
                    edge.explanation().clone().with_optional_evidence(None),
                );
                if let Some(provenance) = edge.provenance() {
                    header = header.with_provenance(provenance.clone());
                }
                header
            } else {
                edge.clone()
            }
        })
        .collect();
    Ok(KmpBundle::new(
        bundle.root_node_id().clone(),
        bundle.role().clone(),
        root,
        neighbors,
        relations,
        Vec::new(),
        bundle.metadata().clone(),
    )?)
}

fn header(node: &BundleNode) -> BundleNode {
    let source = matches!(node.node_kind(), "memory_evidence" | "evidence");
    BundleNode::new(
        node.node_id(),
        node.node_kind(),
        if source { "" } else { node.title() },
        if source { "" } else { node.summary() },
        node.status(),
        node.labels().to_vec(),
        Default::default(),
    )
}
