use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use kmp_application::MemoryAnswerPolicy;
use kmp_proto::v1beta1::AskResponse;

use super::anchor_selection::AnchorSelection;
use super::answer_selection::{RESTATED_FROM_KEY, was_reached_indirectly};
use super::indexed_ask::IndexedAsk;
use super::lexical_bridge::LexicalBridge;
use super::lexical_collection::LexicalCollection;
use super::lexical_reach::LexicalReach;
use super::lexical_row::LexicalRow;
use super::lexicon::Lexicon;
use super::morphology::Morphology;
use super::question_contract::QuestionContract;
use super::row_field::RowField;
use super::search_terms::{
    concept_count, concept_key, informative_terms, strict_answer_focus_terms,
};

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
/// Under the anchored gate the prefixes are the gate's ranking's, and every
/// candidate naming an anchor is read. Not read (`None`, every candidate is
/// read) where the question has no informative word (the ranker then returns
/// every candidate).
#[derive(Debug, Clone, Default)]
pub struct RankPrefixes {
    prefixes: BTreeMap<String, RankPrefix>,
    anchors: BTreeSet<String>,
    /// The candidates no reading of the question can find eligible: below
    /// the floor, or short of the focus a strict policy requires even with
    /// every word their graph can lend them ([`LexicalReach`]).
    ineligible: BTreeSet<String>,
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
        reach: &BTreeMap<String, LexicalReach>,
    ) -> Option<Self> {
        let morphology = Morphology::for_language(indexed.language.as_deref());
        let contract = gated.then(|| QuestionContract::read(question, &morphology));
        let asked = contract
            .as_ref()
            .and_then(QuestionContract::asked)
            .unwrap_or(question);
        if informative_terms(asked, &morphology).is_empty() {
            return None;
        }
        let collection = Arc::new(LexicalCollection::from_indexed(indexed.stats(gated)));
        // A question whose anchors the gate requires is ranked as the gate
        // ranks it: once an anchor decides, the question without its
        // facets, the focus counted but not required. Every candidate that
        // names an anchor is read (`anchors`): the gate cites among them in
        // rank order, and its proof keeps them.
        let mut anchors = BTreeSet::new();
        let mut anchored = false;
        let asked = match contract
            .as_ref()
            .filter(|contract| contract.requires_anchors())
        {
            Some(contract) => {
                anchors = contract
                    .anchors()
                    .iter()
                    .map(|anchor| anchor.term.clone())
                    .collect();
                match AnchorSelection::read(
                    contract,
                    |term| collection.content.document_frequency(term),
                    collection.content.documents(),
                ) {
                    AnchorSelection::Anchored { .. } => {
                        anchored = true;
                        contract.anchored_asked().unwrap_or(question)
                    }
                    _ => asked,
                }
            }
            None => asked,
        };
        if informative_terms(asked, &morphology).is_empty() {
            return None;
        }
        let seed = indexed.seed_documents(gated);
        let lexicon = Lexicon::build(
            asked,
            &morphology,
            &[],
            bridge,
            collection,
            indexed.vocabulary.as_deref().map(Vec::as_slice),
            seed.as_deref(),
        );
        let focus = match policy {
            MemoryAnswerPolicy::EvidenceOrUnknown | MemoryAnswerPolicy::ShowConflicts => {
                Some(strict_answer_focus_terms(asked, &morphology))
            }
            MemoryAnswerPolicy::BestEffort => None,
        };
        // The focus a strict policy requires, as the ranker counts it: two
        // thirds of the focus concepts, none once an anchor decides.
        let required = focus
            .as_ref()
            .filter(|_| !anchored)
            .map(|focus| (concept_count(focus) * 2).div_ceil(3))
            .unwrap_or(0);
        let mut ineligible = BTreeSet::new();
        // The focus concepts, to count a candidate's matches without
        // collecting its terms (`matching_term_count` over its content).
        let concepts = focus.as_ref().map(|focus| {
            focus
                .iter()
                .map(|term| concept_key(term))
                .collect::<BTreeSet<_>>()
        });
        let prefixes = rows
            .map(|(doc, row)| {
                // Read in place: the counts `content_counts` and
                // `direct_counts` collect, without collecting them.
                let content = RowField::content(row, gated);
                let direct = RowField::direct(row, gated);
                let focus_matches = concepts.as_ref().map_or(0, |concepts| {
                    content
                        .terms()
                        .map(|term| concept_key(term))
                        .filter(|key| concepts.contains(key))
                        .collect::<BTreeSet<_>>()
                        .len()
                });
                // Eligibility asks the floor of the direct field, and of a
                // strict policy its focus over the direct field, the claims
                // and the relations; a reach bounds the last two.
                let can_answer = lexicon.clears_floor_of(&direct)
                    && (required == 0
                        || focus.as_ref().is_none_or(|focus| match reach.get(doc) {
                            Some(reach) => {
                                // The reach leaves out the plain direct
                                // field's words, so those are counted too.
                                let plain = RowField::direct(row, false);
                                lexicon.focus_bound(
                                    focus,
                                    direct.terms().chain(plain.terms()).chain(reach.terms()),
                                    &direct,
                                ) >= required
                            }
                            None => true,
                        }));
                if !can_answer {
                    ineligible.insert(doc.clone());
                }
                let prefix = (
                    focus_matches,
                    lexicon.content_score_of(&content),
                    lexicon.direct_score_of(&direct),
                );
                (doc.clone(), prefix)
            })
            .collect();
        Some(Self {
            prefixes,
            anchors,
            ineligible,
        })
    }

    /// The question's anchor terms, when the gate may decide by one: every
    /// candidate carrying one is read.
    pub fn anchors(&self) -> &BTreeSet<String> {
        &self.anchors
    }

    /// Whether no reading of the question can find `doc` eligible: top-k
    /// need not read it for the ranking, and its prefix bounds nothing.
    pub fn cannot_answer(&self, doc: &str) -> bool {
        self.ineligible.contains(doc)
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
    /// How many eligible candidates read `response` carried (the window and
    /// the tail it read, never a rescue): a reading that carried the whole
    /// ranking of what it read found no more than that many.
    pub fn carried(response: &AskResponse) -> usize {
        Self::eligible_ids(response).count()
    }

    /// The eligible candidates `response` carried (the window and the tail
    /// it read, never a rescue), by id.
    pub fn eligible_ids(response: &AskResponse) -> impl Iterator<Item = &str> {
        response
            .proof
            .as_ref()
            .map(|proof| proof.evidence.as_slice())
            .unwrap_or_default()
            .iter()
            .filter(|item| {
                !was_reached_indirectly(item) && !item.metadata.contains_key(RESTATED_FROM_KEY)
            })
            .map(|item| item.id.as_str())
    }

    /// Every candidate no reading of the question can find eligible.
    pub fn unanswerable(&self) -> &BTreeSet<String> {
        &self.ineligible
    }

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
