use std::collections::BTreeMap;

use kmp_domain::{KmpBundle, RelationSignal};

/// The strongest relation the writer attached to each memory in a bundle.
///
/// Wake has no question, so nothing about the caller can order what it
/// returns; what it does have is the judgment already stored on every edge.
/// When `max_entries` cuts, this is what decides whether the survivors are
/// the decisions someone proved or whichever nodes the traversal emitted
/// first.
#[derive(Debug, Default)]
pub(super) struct RelationSignalIndex {
    strongest_by_ref: BTreeMap<String, u32>,
    by_edge: BTreeMap<(String, String, String), u32>,
}

impl RelationSignalIndex {
    pub(super) fn read(bundle: &KmpBundle) -> Self {
        let mut strongest_by_ref = BTreeMap::<String, u32>::new();
        let mut by_edge = BTreeMap::new();
        for relationship in bundle.relationships() {
            let weight =
                RelationSignal::read(relationship.relationship_type(), relationship.explanation())
                    .weight();
            if weight == 0 {
                continue;
            }
            by_edge.insert(
                (
                    relationship.source_node_id().to_string(),
                    relationship.target_node_id().to_string(),
                    relationship.relationship_type().to_string(),
                ),
                weight,
            );
            for endpoint in [relationship.source_node_id(), relationship.target_node_id()] {
                strongest_by_ref
                    .entry(endpoint.to_string())
                    .and_modify(|current| *current = (*current).max(weight))
                    .or_insert(weight);
            }
        }
        Self {
            strongest_by_ref,
            by_edge,
        }
    }

    /// What one specific relation is worth.
    ///
    /// Ordering edges by the strength of their endpoints would say nothing:
    /// two edges into the same well-proven memory would tie, which is exactly
    /// the case the causal spine has to separate.
    pub(super) fn strength_of_edge(&self, source: &str, target: &str, relation: &str) -> u32 {
        self.by_edge
            .get(&(source.to_string(), target.to_string(), relation.to_string()))
            .copied()
            .unwrap_or_default()
    }

    /// What the strongest relation touching this memory is worth. Zero when
    /// nothing proven touches it — which is the honest answer for a memory
    /// held in place by containment alone.
    pub(super) fn strength_of(&self, memory_ref: &str) -> u32 {
        self.strongest_by_ref
            .get(memory_ref)
            .copied()
            .unwrap_or_default()
    }

    /// The strongest relation touching any of a response item's references.
    pub(super) fn strength_over<'a>(&self, refs: impl IntoIterator<Item = &'a str>) -> u32 {
        refs.into_iter()
            .map(|memory_ref| self.strength_of(memory_ref))
            .max()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap as StdMap;

    use kmp_domain::{
        BundleMetadata, BundleNode, BundleRelationship, CaseId, RelationExplanation,
        RelationSemanticClass, Role,
    };

    use super::*;

    fn node(id: &str) -> BundleNode {
        BundleNode::new(
            id,
            "memory",
            id,
            "fixture",
            "ACTIVE",
            Vec::new(),
            StdMap::new(),
        )
    }

    fn bundle(relationships: Vec<BundleRelationship>, refs: &[&str]) -> KmpBundle {
        KmpBundle::new(
            CaseId::new("about:memory").expect("case id"),
            Role::new("resumer").expect("role"),
            node("about:memory"),
            refs.iter().map(|id| node(id)).collect(),
            relationships,
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("valid bundle")
    }

    #[test]
    fn a_proven_decision_outweighs_a_memory_held_by_containment_alone() {
        let index = RelationSignalIndex::read(&bundle(
            vec![
                BundleRelationship::new(
                    "claim:decided",
                    "claim:cause",
                    "triggers",
                    RelationExplanation::new(RelationSemanticClass::Causal)
                        .with_rationale("the reserve was diverted")
                        .with_evidence("change record")
                        .with_confidence("high"),
                ),
                BundleRelationship::new(
                    "scope:timeline",
                    "claim:bookkeeping",
                    "contains_entry",
                    RelationExplanation::new(RelationSemanticClass::Structural),
                ),
            ],
            &[
                "claim:decided",
                "claim:cause",
                "scope:timeline",
                "claim:bookkeeping",
            ],
        ));

        assert!(index.strength_of("claim:decided") > index.strength_of("claim:bookkeeping"));
        assert_eq!(index.strength_of("claim:bookkeeping"), 0);
        assert_eq!(index.strength_of("claim:never-seen"), 0);
    }

    #[test]
    fn both_endpoints_of_a_proven_relation_carry_its_strength() {
        let index = RelationSignalIndex::read(&bundle(
            vec![BundleRelationship::new(
                "claim:effect",
                "claim:cause",
                "triggers",
                RelationExplanation::new(RelationSemanticClass::Causal)
                    .with_rationale("why")
                    .with_evidence("proof")
                    .with_confidence("high"),
            )],
            &["claim:effect", "claim:cause"],
        ));

        assert_eq!(
            index.strength_of("claim:effect"),
            index.strength_of("claim:cause")
        );
        assert!(index.strength_of("claim:cause") > 0);
    }

    #[test]
    fn a_memory_keeps_the_strongest_relation_that_touches_it() {
        let index = RelationSignalIndex::read(&bundle(
            vec![
                BundleRelationship::new(
                    "claim:hub",
                    "claim:weak",
                    "chosen_because",
                    RelationExplanation::new(RelationSemanticClass::Motivational)
                        .with_rationale("why")
                        .with_evidence("proof"),
                ),
                BundleRelationship::new(
                    "claim:hub",
                    "claim:strong",
                    "triggers",
                    RelationExplanation::new(RelationSemanticClass::Causal)
                        .with_rationale("why")
                        .with_evidence("proof")
                        .with_confidence("high"),
                ),
            ],
            &["claim:hub", "claim:weak", "claim:strong"],
        ));

        assert_eq!(
            index.strength_of("claim:hub"),
            index.strength_of("claim:strong")
        );
        assert!(index.strength_of("claim:hub") > index.strength_of("claim:weak"));
        assert_eq!(
            index.strength_over(["claim:weak", "claim:strong"]),
            index.strength_of("claim:strong")
        );
    }
}
