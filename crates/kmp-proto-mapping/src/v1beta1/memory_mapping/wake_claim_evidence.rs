//! Joins a wake claim to the stored sources behind its relation.
//!
//! A relation's `evidence` is prose the writer attached to the edge. The
//! wake used to copy that prose into the claim's reference field, where the
//! projection looked for a `proof.evidence[].id`; prose never matched an id,
//! so the source with its provenance was deferred while its body sat in the
//! spine without one. The join here reads identity from the graph instead:
//! a source counts for a hop when it is one of the hop's endpoints, or when a
//! `supports` edge ties it to an endpoint and it holds the hop's evidence
//! text. Equal text elsewhere in the about is a different source.

use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::KmpBundle;
use kmp_proto::v1beta1::{MemoryEvidence, MemoryRelation, WakeClaim};

const STORED_EVIDENCE: &str = "stored_evidence";

pub(super) struct WakeClaimEvidence<'a> {
    by_node: BTreeMap<&'a str, &'a MemoryEvidence>,
    supported_by: BTreeMap<&'a str, Vec<&'a str>>,
}

impl<'a> WakeClaimEvidence<'a> {
    /// `evidence` is the selection the packet returns, so every ref this
    /// emits resolves inside the same `proof.evidence`.
    pub(super) fn new(bundle: &'a KmpBundle, evidence: &'a [MemoryEvidence]) -> Self {
        let by_node = evidence
            .iter()
            .filter(|item| {
                item.metadata.get("proof_role").map(String::as_str) == Some(STORED_EVIDENCE)
            })
            .filter_map(|item| Some((item.id.strip_prefix("detail:")?, item)))
            .collect::<BTreeMap<_, _>>();
        let mut supported_by = BTreeMap::<&str, Vec<&str>>::new();
        for relationship in bundle
            .relationships()
            .iter()
            .filter(|relationship| relationship.relationship_type() == "supports")
            .filter(|relationship| by_node.contains_key(relationship.source_node_id()))
        {
            supported_by
                .entry(relationship.target_node_id())
                .or_default()
                .push(relationship.source_node_id());
        }
        Self {
            by_node,
            supported_by,
        }
    }

    pub(super) fn claim(&self, relationship: &MemoryRelation) -> WakeClaim {
        let endpoints = [
            relationship.source_ref.as_str(),
            relationship.target_ref.as_str(),
        ];
        let text = relationship.evidence.as_str();
        let mut refs = BTreeSet::new();
        let mut text_held = false;
        for endpoint in endpoints {
            if let Some(item) = self.by_node.get(endpoint) {
                refs.insert(item.id.clone());
                text_held |= !text.is_empty() && item.text == text;
            }
            for source in self.supported_by.get(endpoint).into_iter().flatten() {
                let item = self.by_node[source];
                if !text.is_empty() && item.text == text {
                    refs.insert(item.id.clone());
                    text_held = true;
                }
            }
        }
        WakeClaim {
            claim: format!("{} -> {}", relationship.source_ref, relationship.target_ref),
            because: if relationship.why.is_empty() {
                "Kernel relationship path selected this edge.".to_string()
            } else {
                relationship.why.clone()
            },
            evidence_refs: refs.into_iter().collect(),
            evidence: if text_held {
                String::new()
            } else {
                text.to_string()
            },
        }
    }
}
