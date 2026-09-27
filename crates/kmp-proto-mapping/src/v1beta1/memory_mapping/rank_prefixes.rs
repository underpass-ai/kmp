use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use kmp_application::MemoryAnswerPolicy;
use kmp_proto::v1beta1::AskResponse;

use super::answer_selection::{RESTATED_FROM_KEY, was_reached_indirectly};
use super::indexed_ask::IndexedAsk;
use super::lexical_bridge::LexicalBridge;
use super::lexical_collection::LexicalCollection;
use super::lexical_row::LexicalRow;
use super::lexicon::Lexicon;
use super::morphology::Morphology;
use super::question_contract::QuestionContract;
use super::search_terms::{informative_terms, matching_term_count, strict_answer_focus_terms};

/// The leading part of a candidate's `RelevanceKey`: focus matches in its
/// content, then its content and direct BM25, in tenths. Everything below
/// it (claims, relations, recency) only orders candidates whose prefixes tie.
pub type RankPrefix = (usize, i64, i64);

/// Every candidate's exact [`RankPrefix`] from the lexical index's rows, for
/// top-k over the index (DESIGN L6, P14).
///
/// The ranking sorts eligible candidates by their `RelevanceKey`, whose
/// leading fields depend only on a candidate's counts, the whole about's
/// statistics and the question's weights: the same integers the index holds
/// give the same prefix to the bit. So a reading that carries the head and
/// a depth of the tail needs only the candidates whose prefix can reach
/// them; [`Self::certifies`] says, from the ranking the read candidates gave,
/// whether none left unread could have.
///
/// Not read (`None`, every candidate is read) where the gate may decide by
/// an anchor (it ranks without the focus filter and cites by anchor), and
/// where the question has no informative word (the ranker then returns
/// every candidate).
#[derive(Debug, Clone, Default)]
pub struct RankPrefixes {
    prefixes: BTreeMap<String, RankPrefix>,
}

impl RankPrefixes {
    /// The prefixes of `rows` (candidate doc, row) for `question` read as the
    /// ranker will: under the gate's reading when `gated`, with `policy`'s
    /// focus, against `indexed`.
    pub fn read<'r>(
        question: &str,
        policy: MemoryAnswerPolicy,
        gated: bool,
        indexed: &IndexedAsk,
        bridge: &LexicalBridge,
        rows: impl Iterator<Item = (&'r String, &'r LexicalRow)>,
    ) -> Option<Self> {
        let morphology = Morphology::for_language(indexed.language.as_deref());
        let contract = gated.then(|| QuestionContract::read(question, &morphology));
        if contract
            .as_ref()
            .is_some_and(QuestionContract::requires_anchors)
        {
            return None;
        }
        let asked = contract
            .as_ref()
            .and_then(QuestionContract::asked)
            .unwrap_or(question);
        if informative_terms(asked, &morphology).is_empty() {
            return None;
        }
        let collection = Arc::new(LexicalCollection::from_indexed(indexed.stats(gated)));
        let seed = indexed.seed_documents(gated);
        let lexicon = Lexicon::build(
            asked,
            &morphology,
            &[],
            bridge,
            collection,
            indexed.vocabulary.as_deref().map(Vec::as_slice),
            seed.as_deref().map(Vec::as_slice),
        );
        let focus = match policy {
            MemoryAnswerPolicy::EvidenceOrUnknown | MemoryAnswerPolicy::ShowConflicts => {
                Some(strict_answer_focus_terms(asked, &morphology))
            }
            MemoryAnswerPolicy::BestEffort => None,
        };
        let prefixes = rows
            .map(|(doc, row)| {
                let content = row.content_counts(gated);
                let focus_matches = focus.as_ref().map_or(0, |focus| {
                    matching_term_count(focus, &content.terms().cloned().collect::<BTreeSet<_>>())
                });
                let prefix = (
                    focus_matches,
                    lexicon.content_score_of(&content),
                    lexicon.direct_score_of(&row.direct_counts(gated)),
                );
                (doc.clone(), prefix)
            })
            .collect();
        Some(Self { prefixes })
    }

    /// A candidate's prefix.
    pub fn prefix(&self, doc: &str) -> Option<RankPrefix> {
        self.prefixes.get(doc).copied()
    }

    /// Every candidate with its prefix, best first (ties by doc).
    pub fn ordered(&self) -> Vec<(RankPrefix, String)> {
        let mut ordered = self
            .prefixes
            .iter()
            .map(|(doc, prefix)| (*prefix, doc.clone()))
            .collect::<Vec<_>>();
        ordered.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        ordered
    }

    /// Whether `response`, ranked from the candidates read, is the one the
    /// whole about gives, when the best candidate left unread has the prefix
    /// `unread` (`None`: nothing was left unread).
    ///
    /// It is when the reading did not carry the whole ranking and every
    /// eligible candidate it carried (the head's window and the tail it
    /// read, never a rescue) ranks strictly above `unread` on the prefix
    /// alone: an unread candidate can then only fall after all of them,
    /// beyond the depth read, and the head (built from the window) is the
    /// same. A rescue walks from the window and is read with it.
    pub fn certifies(&self, response: &AskResponse, unread: Option<RankPrefix>) -> bool {
        let Some(unread) = unread else {
            return true;
        };
        if !response.more_ranked {
            return false;
        }
        let evidence = response
            .proof
            .as_ref()
            .map(|proof| proof.evidence.as_slice())
            .unwrap_or_default();
        let mut lowest: Option<RankPrefix> = None;
        for item in evidence {
            if was_reached_indirectly(item) || item.metadata.contains_key(RESTATED_FROM_KEY) {
                continue;
            }
            let Some(prefix) = self.prefix(&item.id) else {
                return false;
            };
            lowest = Some(lowest.map_or(prefix, |lowest| lowest.min(prefix)));
        }
        lowest.is_some_and(|lowest| unread < lowest)
    }
}
