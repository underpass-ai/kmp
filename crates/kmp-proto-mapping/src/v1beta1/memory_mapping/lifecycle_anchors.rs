use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::LifecycleChain;
use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate_terms::AnswerCandidateTerms;
use super::answer_selection::{REACHED_BY_KEY, REACHED_BY_LIFECYCLE, answer_context_refs};
use super::bundle_lifecycle_links::BundleLifecycleLinks;
use super::memory_lifecycle::MemoryLifecycle;

/// Which memories named each of a question's anchors, read before ranking,
/// so a memory the lifecycle rescue reached from one of them is known to be
/// about the anchor too.
///
/// The anchored gate keeps, in a proof it did not answer whole, only the
/// memories about its anchors. The current head of a replaced memory that
/// named the anchor is about it: the writer declared it took over. Without
/// this, a question whose only match was replaced would come back UNKNOWN
/// with the replacement filtered out of its proof.
#[derive(Debug, Default)]
pub(super) struct LifecycleAnchors {
    /// Memory ref -> the anchor terms its content names.
    named_by: BTreeMap<String, BTreeSet<String>>,
}

impl LifecycleAnchors {
    /// Reads the candidates only when the bundle declares a lifecycle.
    pub(super) fn read<'a>(
        declares_lifecycle: bool,
        anchors: &BTreeSet<String>,
        candidates: impl IntoIterator<Item = (&'a MemoryEvidence, &'a AnswerCandidateTerms)>,
    ) -> Self {
        let mut named_by = BTreeMap::<String, BTreeSet<String>>::new();
        if !declares_lifecycle {
            return Self { named_by };
        }
        for (item, terms) in candidates {
            let named = anchors
                .iter()
                .filter(|anchor| terms.content_counts.count(anchor) > 0)
                .cloned()
                .collect::<BTreeSet<_>>();
            if named.is_empty() {
                continue;
            }
            for node in answer_context_refs(item) {
                named_by
                    .entry(node)
                    .or_default()
                    .extend(named.iter().cloned());
            }
        }
        Self { named_by }
    }

    /// Whether `item` was reached along a lifecycle from a memory that
    /// names any anchor.
    pub(super) fn reached_from_an_anchor(&self, item: &MemoryEvidence) -> bool {
        self.origin(item).is_some()
    }

    /// For the measured variant: each still-standing head of a replaced
    /// memory that named `principal`, with the memory it stands in for and
    /// the relation that reached it. The head may be a memory the question
    /// matched in its own words or one the rescue brought; either way the
    /// writer declared it took over from a memory about the anchor.
    pub(super) fn standing_heads(
        &self,
        principal: &str,
        links: &BundleLifecycleLinks,
        lifecycle: &MemoryLifecycle,
    ) -> BTreeMap<String, (String, &'static str)> {
        let stands = |node: &str| !lifecycle.is_superseded(node) && !lifecycle.is_expired(node);
        let mut heads = BTreeMap::new();
        for (from, named) in &self.named_by {
            if !named.contains(principal) || stands(from) {
                continue;
            }
            let Ok(chain) = LifecycleChain::successors(from, links) else {
                continue;
            };
            for head in chain.heads().iter().filter(|head| stands(head)) {
                let via = chain
                    .newer()
                    .iter()
                    .find(|step| &step.node == head)
                    .map(|step| step.via.as_str());
                if let Some(via) = via {
                    heads
                        .entry(head.clone())
                        .or_insert_with(|| (from.clone(), via));
                }
            }
        }
        heads
    }

    fn origin<'i>(&self, item: &'i MemoryEvidence) -> Option<&'i str> {
        if item.metadata.get(REACHED_BY_KEY).map(String::as_str) != Some(REACHED_BY_LIFECYCLE) {
            return None;
        }
        let from = item.metadata.get("reached_from")?;
        self.named_by.contains_key(from).then_some(from.as_str())
    }
}
