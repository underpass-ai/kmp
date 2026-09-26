//! Looking a bundle node up by id without walking the node list per lookup.
//!
//! Mapping a bundle reads a node's persisted properties once per evidence
//! detail and once per entry; a linear search there made every recall
//! quadratic in the size of the about.

use std::collections::{BTreeMap, HashMap};

use kmp_domain::{BundleNode, KmpBundle};

use super::bundle_views::persisted_memory_metadata;

pub(super) struct BundleNodeIndex<'a> {
    nodes: HashMap<&'a str, &'a BundleNode>,
}

impl<'a> BundleNodeIndex<'a> {
    /// Indexes the root and its neighbours. A repeated id keeps its first
    /// node, the one a front-to-back search would have found.
    pub(super) fn new(bundle: &'a KmpBundle) -> Self {
        let mut nodes = HashMap::with_capacity(bundle.neighbor_nodes().len() + 1);
        for node in std::iter::once(bundle.root_node()).chain(bundle.neighbor_nodes()) {
            nodes.entry(node.node_id()).or_insert(node);
        }
        Self { nodes }
    }

    pub(super) fn node(&self, node_id: &str) -> Option<&'a BundleNode> {
        self.nodes.get(node_id).copied()
    }

    pub(super) fn properties(&self, node_id: &str) -> Option<&'a BTreeMap<String, String>> {
        self.node(node_id).map(BundleNode::properties)
    }

    pub(super) fn memory_metadata(&self, node_id: &str) -> HashMap<String, String> {
        self.properties(node_id)
            .map(persisted_memory_metadata)
            .unwrap_or_default()
    }
}
