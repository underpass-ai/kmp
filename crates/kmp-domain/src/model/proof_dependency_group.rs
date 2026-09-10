use std::collections::{BTreeSet, VecDeque};

use crate::{BundleRelationship, RelationSemanticClass};

/// A bounded neighborhood of writer-declared, evidenced memory connections.
///
/// This describes structural coverage, never semantic sufficiency or truth.
/// The caller supplies the scoped memories and clock-admitted relationships.
/// Ineligible endpoint identities never leave this result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofDependencyGroup {
    seed_ref: String,
    member_refs: Vec<String>,
    unavailable_in_selection: usize,
    omitted_by_limit: usize,
}

impl ProofDependencyGroup {
    pub const MAX_HOPS: usize = 2;
    pub const MAX_MEMBERS: usize = 8;

    pub fn select(
        seed: &str,
        scoped_entries: &BTreeSet<String>,
        eligible_entries: &BTreeSet<String>,
        relationships: &[BundleRelationship],
    ) -> Self {
        let mut members = BTreeSet::new();
        let mut order = Vec::new();
        let mut queue = VecDeque::new();
        let mut unavailable = BTreeSet::new();
        let mut omitted = BTreeSet::new();
        if eligible_entries.contains(seed) {
            members.insert(seed.to_string());
            order.push(seed.to_string());
            queue.push_back((seed.to_string(), 0));
        }
        while let Some((current, hops)) = queue.pop_front() {
            let neighbors = relationships
                .iter()
                .filter(|edge| evidenced_memory_link(edge, scoped_entries))
                .filter_map(|edge| {
                    if edge.source_node_id() == current {
                        Some(edge.target_node_id())
                    } else if edge.target_node_id() == current {
                        Some(edge.source_node_id())
                    } else {
                        None
                    }
                })
                .collect::<BTreeSet<_>>();
            for target in neighbors {
                if members.contains(target) {
                    continue;
                }
                if !eligible_entries.contains(target) {
                    unavailable.insert(target.to_string());
                } else if hops == Self::MAX_HOPS || members.len() == Self::MAX_MEMBERS {
                    omitted.insert(target.to_string());
                } else {
                    members.insert(target.to_string());
                    order.push(target.to_string());
                    queue.push_back((target.to_string(), hops + 1));
                }
            }
        }
        // A node omitted along a longer route can still be reached through a
        // shorter route encountered later in the deterministic traversal.
        omitted.retain(|target| !members.contains(target));
        Self {
            seed_ref: seed.to_string(),
            member_refs: order,
            unavailable_in_selection: unavailable.len(),
            omitted_by_limit: omitted.len(),
        }
    }

    pub fn seed_ref(&self) -> &str {
        &self.seed_ref
    }

    pub fn member_refs(&self) -> &[String] {
        &self.member_refs
    }

    pub fn unavailable_in_selection(&self) -> usize {
        self.unavailable_in_selection
    }

    pub fn omitted_by_limit(&self) -> usize {
        self.omitted_by_limit
    }
}

fn evidenced_memory_link(edge: &BundleRelationship, entries: &BTreeSet<String>) -> bool {
    let explanation = edge.explanation();
    entries.contains(edge.source_node_id())
        && entries.contains(edge.target_node_id())
        && *explanation.semantic_class() != RelationSemanticClass::Structural
        && explanation
            .rationale()
            .is_some_and(|text| !text.trim().is_empty())
        && explanation
            .evidence()
            .is_some_and(|text| !text.trim().is_empty())
}
