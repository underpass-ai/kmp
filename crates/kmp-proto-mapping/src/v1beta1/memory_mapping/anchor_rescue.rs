use std::collections::{BTreeMap, BTreeSet};

use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate_terms::AnswerCandidateTerms;
use super::answer_selection::answer_context_refs;
use super::reach_graph::ReachGraph;

/// Which candidates name a question's anchor through a declared
/// `same_entity_as` rather than in their own words.
///
/// A writer who declared two memories the same entity said that what one
/// names, the other is about: an entry on `issue 185` and one on the PR that
/// closed it, declared the same thing, both answer for `#185`. The rescue is
/// one hop, only from a candidate that names the anchor itself, and every
/// memory it admits is marked with the ref it stands in for. "Names" is
/// read in the candidate's content, as the gate reads everything else.
#[derive(Debug, Default)]
pub(super) struct AnchorRescue {
    /// Candidate id -> anchor term -> the ref that names it.
    rescued: BTreeMap<String, BTreeMap<String, String>>,
}

impl AnchorRescue {
    pub(super) fn read(
        anchors: &BTreeSet<String>,
        candidates: &[(&MemoryEvidence, AnswerCandidateTerms)],
        graph: &ReachGraph,
    ) -> Self {
        let mut rescued = BTreeMap::<String, BTreeMap<String, String>>::new();
        if !graph.has_equivalences() {
            return Self { rescued };
        }
        for term in anchors {
            let naming = candidates
                .iter()
                .filter(|(_, terms)| terms.content_counts.count(term) > 0)
                .flat_map(|(item, _)| answer_context_refs(item))
                .collect::<BTreeSet<_>>();
            if naming.is_empty() {
                continue;
            }
            for (item, terms) in candidates {
                if terms.content_counts.count(term) > 0 {
                    continue;
                }
                let from = answer_context_refs(item).iter().find_map(|item_ref| {
                    graph
                        .same_entity_as(item_ref)
                        .find(|other| naming.contains(*other))
                        .map(str::to_string)
                });
                if let Some(from) = from {
                    rescued
                        .entry(item.id.clone())
                        .or_default()
                        .insert(term.clone(), from);
                }
            }
        }
        Self { rescued }
    }

    /// Whether a candidate names `term`, in its own words or by rescue.
    ///
    /// Its own words are its content: a source, a ref or a metadata value
    /// that spells the anchor does not make the memory about it.
    pub(super) fn names(
        &self,
        item: &MemoryEvidence,
        terms: &AnswerCandidateTerms,
        term: &str,
    ) -> bool {
        terms.content_counts.count(term) > 0
            || self
                .rescued
                .get(&item.id)
                .is_some_and(|rescued| rescued.contains_key(term))
    }

    /// The ref a rescued candidate stands in for, if it was rescued.
    pub(super) fn standing_in_for(&self, id: &str) -> Option<&str> {
        self.rescued
            .get(id)
            .and_then(|rescued| rescued.values().next())
            .map(String::as_str)
    }
}
