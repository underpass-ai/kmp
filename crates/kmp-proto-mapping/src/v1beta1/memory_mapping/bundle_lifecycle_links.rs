use std::collections::BTreeMap;

use kmp_domain::{
    KmpBundle, LifecycleLink, LifecycleLinkSource, LifecycleNeighbours, LifecycleRelation,
    PortError,
};

/// The lifecycle links a bundle declares, indexed both ways.
///
/// Ask reads a bundle it already holds, so a chain walks these maps in
/// O(D·log R) instead of the store's index; the walk itself is the one the
/// store uses ([`kmp_domain::LifecycleChain`]).
#[derive(Debug, Default)]
pub(super) struct BundleLifecycleLinks {
    newer_than: BTreeMap<String, Vec<LifecycleLink>>,
    older_than: BTreeMap<String, Vec<LifecycleLink>>,
}

impl BundleLifecycleLinks {
    pub(super) fn from_bundle(bundle: &KmpBundle) -> Self {
        let mut links = Self::default();
        for relationship in bundle.relationships() {
            let Some(relation) = LifecycleRelation::parse(relationship.relationship_type()) else {
                continue;
            };
            if relationship.source_node_id() == relationship.target_node_id() {
                continue;
            }
            let link = LifecycleLink {
                newer: relationship.source_node_id().to_string(),
                older: relationship.target_node_id().to_string(),
                relation,
                occurred_at: relationship.explanation().occurred_at().map(str::to_string),
            };
            links
                .older_than
                .entry(link.newer.clone())
                .or_default()
                .push(link.clone());
            links
                .newer_than
                .entry(link.older.clone())
                .or_default()
                .push(link);
        }
        links
    }

    /// Whether the bundle declares no lifecycle at all: then nothing about
    /// a lifecycle needs reading.
    pub(super) fn is_empty(&self) -> bool {
        self.newer_than.is_empty()
    }

    /// Whether anything declares it took over from `node`.
    pub(super) fn has_newer(&self, node: &str) -> bool {
        self.newer_than.contains_key(node)
    }

    /// Whether `node` touches any lifecycle link.
    pub(super) fn touches(&self, node: &str) -> bool {
        self.newer_than.contains_key(node) || self.older_than.contains_key(node)
    }
}

impl LifecycleLinkSource for BundleLifecycleLinks {
    fn newer_than(&self, node: &str) -> Result<LifecycleNeighbours, PortError> {
        Ok(LifecycleNeighbours::complete(
            self.newer_than.get(node).cloned().unwrap_or_default(),
        ))
    }

    fn older_than(&self, node: &str) -> Result<LifecycleNeighbours, PortError> {
        Ok(LifecycleNeighbours::complete(
            self.older_than.get(node).cloned().unwrap_or_default(),
        ))
    }
}
