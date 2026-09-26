//! Joining proof hops to the canonical evidence registry without scanning the
//! whole registry once per hop.
//!
//! A hop is incident to an evidence item when the item stands for one of the
//! hop's endpoints, or when it supports an endpoint *and* holds the hop's own
//! `why` or `evidence` text. Both conditions are exact-key lookups, so the
//! registry is indexed once by node ref and by (supported ref, text) and each
//! hop only looks at the items that can possibly join it. The decision per
//! candidate is the same predicate the linear scan applied, and refs are kept
//! in a `BTreeSet`, so the result does not depend on evidence order.

use std::collections::{BTreeSet, HashMap};

use kmp_proto::v1beta1::{MemoryEvidence, MemoryRelation, MemorySemanticClass};

pub(crate) struct ProofEvidenceIndex<'a> {
    evidence: &'a [MemoryEvidence],
    /// `detail:` stripped id → items standing for that node.
    by_node_ref: HashMap<&'a str, Vec<usize>>,
    /// supported ref → evidence text → items that support it with that text.
    by_supported_ref: HashMap<&'a str, HashMap<&'a str, Vec<usize>>>,
}

impl<'a> ProofEvidenceIndex<'a> {
    pub(crate) const CANONICAL_REFS_WHY: &'static str = "Supported by canonical evidence refs.";

    pub(crate) fn new(evidence: &'a [MemoryEvidence]) -> Self {
        let mut by_node_ref: HashMap<&'a str, Vec<usize>> = HashMap::new();
        let mut by_supported_ref: HashMap<&'a str, HashMap<&'a str, Vec<usize>>> = HashMap::new();
        for (index, item) in evidence.iter().enumerate() {
            let node_ref = item.id.strip_prefix("detail:").unwrap_or(&item.id);
            by_node_ref.entry(node_ref).or_default().push(index);
            // An empty body never matches a hop's text, so it can only join
            // through its own node.
            if item.text.is_empty() {
                continue;
            }
            for supported_ref in &item.supports {
                let texts = by_supported_ref.entry(supported_ref.as_str()).or_default();
                let items = texts.entry(item.text.as_str()).or_default();
                if items.last() != Some(&index) {
                    items.push(index);
                }
            }
        }
        Self {
            evidence,
            by_node_ref,
            by_supported_ref,
        }
    }

    /// Evidence items that may be incident to `relation`, sorted and unique.
    fn candidates(&self, relation: &MemoryRelation) -> Vec<usize> {
        let mut candidates = Vec::new();
        for endpoint in [relation.source_ref.as_str(), relation.target_ref.as_str()] {
            if let Some(items) = self.by_node_ref.get(endpoint) {
                candidates.extend_from_slice(items);
            }
            let Some(texts) = self.by_supported_ref.get(endpoint) else {
                continue;
            };
            for text in [relation.why.as_str(), relation.evidence.as_str()] {
                if text.is_empty() {
                    continue;
                }
                if let Some(items) = texts.get(text) {
                    candidates.extend_from_slice(items);
                }
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        candidates
    }

    /// Replaces hop bodies that repeat canonical evidence with stable refs and
    /// records every incident evidence ref on the hop.
    pub(crate) fn normalize(&self, relation: &mut MemoryRelation) {
        let mut refs = relation
            .evidence_refs
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut repeated_why = false;
        let mut repeated_evidence = false;

        for index in self.candidates(relation) {
            let item = &self.evidence[index];
            let evidence_node_ref = item.id.strip_prefix("detail:").unwrap_or(&item.id);
            let why_matches = !relation.why.is_empty() && relation.why == item.text;
            let evidence_matches = !relation.evidence.is_empty() && relation.evidence == item.text;
            let endpoint = relation.source_ref == evidence_node_ref
                || relation.target_ref == evidence_node_ref;
            let supports_endpoint = item.supports.iter().any(|supported_ref| {
                relation.source_ref == *supported_ref || relation.target_ref == *supported_ref
            });
            // A source that merely supports an endpoint backs that memory, not
            // this hop; it joins the hop only when it holds the hop's own text.
            let incident = endpoint || (supports_endpoint && (why_matches || evidence_matches));

            // Equal text proves nothing about provenance: a body only joins a
            // hop through a source the graph ties to one of its endpoints.
            if incident {
                refs.insert(item.id.clone());
                repeated_why |= why_matches;
                repeated_evidence |= evidence_matches;
            }
        }

        if repeated_why {
            relation.why.clear();
        }
        if repeated_evidence {
            relation.evidence.clear();
        }
        relation.evidence_refs = refs.into_iter().collect();
        if relation.semantic_class != MemorySemanticClass::Structural as i32
            && relation.why.is_empty()
            && relation.evidence.is_empty()
            && !relation.evidence_refs.is_empty()
        {
            relation.why = Self::CANONICAL_REFS_WHY.to_string();
        }
    }
}

#[cfg(test)]
#[path = "proof_evidence_index_tests.rs"]
mod tests;
