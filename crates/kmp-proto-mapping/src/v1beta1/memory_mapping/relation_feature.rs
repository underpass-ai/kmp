use std::cmp::Ordering;
use std::collections::BTreeSet;

use kmp_domain::RelationSemanticClass;

use super::search_terms::terms_match;

use super::relation_direction::RelationDirection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RelationFeature {
    pub(super) rel: String,
    pub(super) semantic_class: RelationSemanticClass,
    /// What the writer's own judgment of this edge is worth to retrieval.
    pub(super) signal: u32,
    pub(super) direction: RelationDirection,
    pub(super) other_endpoint_ref: String,
    pub(super) endpoint_terms: BTreeSet<String>,
    pub(super) why_terms: BTreeSet<String>,
    pub(super) evidence_terms: BTreeSet<String>,
    pub(super) relation_terms: BTreeSet<String>,
}

impl RelationFeature {
    pub(super) fn searchable_terms(&self) -> BTreeSet<String> {
        self.endpoint_terms
            .iter()
            .chain(&self.why_terms)
            .chain(&self.evidence_terms)
            .chain(&self.relation_terms)
            .cloned()
            .collect()
    }

    pub(super) fn matches_any(&self, question_terms: &BTreeSet<String>) -> bool {
        let relation_terms = self.searchable_terms();
        question_terms.iter().any(|question_term| {
            relation_terms
                .iter()
                .any(|relation_term| terms_match(question_term, relation_term))
        })
    }

    pub(super) fn stable_cmp(&self, other: &Self) -> Ordering {
        // Signal first, so the sixteen features a candidate keeps are the
        // best-proven ones rather than whichever class happened to sort low.
        other
            .signal
            .cmp(&self.signal)
            .then_with(|| {
                self.semantic_class
                    .salience_rank()
                    .cmp(&other.semantic_class.salience_rank())
            })
            .then_with(|| self.rel.cmp(&other.rel))
            .then_with(|| self.direction.cmp(&other.direction))
            .then_with(|| self.other_endpoint_ref.cmp(&other.other_endpoint_ref))
            .then_with(|| self.endpoint_terms.cmp(&other.endpoint_terms))
            .then_with(|| self.why_terms.cmp(&other.why_terms))
            .then_with(|| self.evidence_terms.cmp(&other.evidence_terms))
            .then_with(|| self.relation_terms.cmp(&other.relation_terms))
    }
}
