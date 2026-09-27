use kmp_domain::{BundleNode, BundleNodeDetail};
use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_recall_context::node_carries_search_summary;
use super::bundle_views::{entry_candidate, evidence_candidate, is_memory_evidence_kind};
use super::language_signals::LanguageSignals;

/// How one stored node reads for `kmp_ask`, one node at a time: the
/// candidates it makes and what it adds to its about's language.
///
/// The ranker reads the same things off a whole bundle
/// (`answer_evidence_from_bundle`, `search_language`); the lexical sidecar
/// reads them node by node through these, which share that code, so a node
/// cannot read one way in a bundle and another way in the index.
pub struct LexicalReading;

impl LexicalReading {
    /// Whether a node kind stores evidence, whose detail is a candidate.
    pub fn is_evidence_kind(kind: &str) -> bool {
        is_memory_evidence_kind(kind)
    }

    /// The candidate an entry's text makes once a selected `contains_entry`
    /// reaches it; none when its text is blank.
    pub fn entry_candidate(node: &BundleNode) -> Option<MemoryEvidence> {
        entry_candidate(node)
    }

    /// The candidate an evidence node's detail makes, supporting `supports`
    /// (the node itself when it supports nothing in its about).
    pub fn evidence_candidate(
        node: &BundleNode,
        detail: &BundleNodeDetail,
        supports: Vec<String>,
    ) -> MemoryEvidence {
        let supports = if supports.is_empty() {
            vec![node.node_id().to_string()]
        } else {
            supports
        };
        evidence_candidate(Some(node.properties()), detail, supports)
    }

    /// What a node's summary and detail add to its about's language signals.
    pub fn node_signals(node: &BundleNode, detail: Option<&BundleNodeDetail>) -> LanguageSignals {
        let mut signals = LanguageSignals::of_texts([node.summary()]);
        if let Some(detail) = detail {
            signals.add(&LanguageSignals::of_texts([detail.detail()]));
        }
        signals
    }

    /// Whether the node carries a linted English search summary.
    pub fn carries_search_summary(node: &BundleNode) -> bool {
        node_carries_search_summary(node)
    }
}
