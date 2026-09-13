use super::temporal_index_identity::TemporalIndexIdentity;
use kmp_domain::{KmpBundle, TemporalMemoryIndex};
use std::sync::Arc;

/// One reusable catalogue, independent of page, interval and label selection.
/// A different identity replaces it; large catalogues are read but not retained.
#[derive(Debug, Default)]
pub(super) struct TemporalIndexCache {
    current: Option<(TemporalIndexIdentity, Arc<TemporalMemoryIndex>)>,
}

impl TemporalIndexCache {
    pub(super) fn get(&self, identity: &TemporalIndexIdentity) -> Option<Arc<TemporalMemoryIndex>> {
        let (stored, index) = self.current.as_ref()?;
        (stored == identity).then(|| Arc::clone(index))
    }

    pub(super) fn put(&mut self, identity: TemporalIndexIdentity, index: Arc<TemporalMemoryIndex>) {
        self.current = cacheable(index.bundle()).then_some((identity, index));
    }
}

const MAX_CATALOGUE_NODES: usize = 8192;
const MAX_CATALOGUE_RELATIONSHIPS: usize = 32768;
const RETENTION_ALLOWANCE: usize = 64 * 1024 * 1024;

// Counts retained catalogue text conservatively, including metadata and
// relation explanations. The factor covers index copies of coordinates and
// container overhead. This is an admission allowance, not measured process RSS.
fn cacheable(bundle: &KmpBundle) -> bool {
    if !bundle.node_details().is_empty()
        || bundle.neighbor_nodes().len() >= MAX_CATALOGUE_NODES
        || bundle.relationships().len() > MAX_CATALOGUE_RELATIONSHIPS
    {
        return false;
    }
    let mut allowance = RETENTION_ALLOWANCE;
    let mut charge = |bytes: usize| {
        if let Some(left) = allowance.checked_sub(bytes.saturating_mul(4)) {
            allowance = left;
            true
        } else {
            false
        }
    };
    for node in std::iter::once(bundle.root_node()).chain(bundle.neighbor_nodes()) {
        if !charge(
            256 + node.node_id().len()
                + node.node_kind().len()
                + node.title().len()
                + node.summary().len()
                + node.status().len(),
        ) {
            return false;
        }
        for value in node.labels() {
            if !charge(value.len()) {
                return false;
            }
        }
        for (key, value) in node.properties() {
            if !charge(64 + key.len() + value.len()) {
                return false;
            }
        }
        if let Some(provenance) = node.provenance()
            && !charge(
                provenance.source_agent().map_or(0, str::len)
                    + provenance.observed_at().map_or(0, str::len),
            )
        {
            return false;
        }
    }
    for relation in bundle.relationships() {
        if !charge(
            256 + relation.source_node_id().len()
                + relation.target_node_id().len()
                + relation.relationship_type().len(),
        ) {
            return false;
        }
        for (key, value) in relation.explanation().to_properties() {
            if !charge(64 + key.len() + value.len()) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use kmp_domain::{BundleMetadata, BundleNode, CaseId, Role};

    fn bundle(summary: String) -> KmpBundle {
        KmpBundle::new(
            CaseId::new("root").expect("fixture operation succeeds"),
            Role::new("reader").expect("fixture operation succeeds"),
            BundleNode::new(
                "root",
                "memory_anchor",
                "root",
                summary,
                "ACTIVE",
                vec![],
                Default::default(),
            ),
            vec![],
            vec![],
            vec![],
            BundleMetadata::initial("cache-test"),
        )
        .expect("fixture operation succeeds")
    }

    #[test]
    fn one_oversized_catalogue_is_read_without_retaining_it() {
        let small = bundle("summary".into());
        let large = bundle("x".repeat(RETENTION_ALLOWANCE / 4));
        assert!(cacheable(&small));
        assert!(!cacheable(&large));
        let id = TemporalIndexIdentity::new(
            kmp_domain::GraphReadRevision::new("test:1").expect("fixture operation succeeds"),
            vec!["root".into()],
            8,
            kmp_domain::TemporalAxis::Observed,
        );
        let mut cache = TemporalIndexCache::default();
        cache.put(
            id.clone(),
            Arc::new(
                TemporalMemoryIndex::new(small, kmp_domain::TemporalAxis::Observed)
                    .expect("fixture operation succeeds"),
            ),
        );
        assert!(cache.get(&id).is_some());
        cache.put(
            id.clone(),
            Arc::new(
                TemporalMemoryIndex::new(large, kmp_domain::TemporalAxis::Observed)
                    .expect("fixture operation succeeds"),
            ),
        );
        assert!(cache.get(&id).is_none());
    }
}
