use std::collections::{BTreeMap, BTreeSet};

use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate_terms::AnswerCandidateTerms;
use super::answer_recall_context::AnswerRecallContext;
use super::answer_selection::{
    EXPANSION_TERMS_KEY, REACHED_BY_EXPANSION, mark_reached_by, stable_evidence_key,
};
use super::candidate_temporal_state::CandidateTemporalState;
use super::lexical_field::{LexicalField, ranked_score};
use super::search_terms::{informative_tokens, matching_term_count, search_key};
use super::term_counts::TermCounts;

/// How many memories the expansions may bring in: as many as the other
/// rescues, and like them after every memory the question reached in its
/// own words.
pub(super) const MAX_EXPANDED_CANDIDATES: usize = 3;

/// A candidate with the terms the ranker read it with.
type ReadCandidate = (MemoryEvidence, AnswerCandidateTerms);

/// Brings in what a question reached only through a memory's judged
/// expansions (Doc2Query--, arXiv 2301.03266).
///
/// The expansions are their own field (X), scored with BM25 over the
/// candidates that carry them and held to the same floor a memory's own
/// words are: the median informativeness of the question's concepts. Under a
/// strict policy the focus must also be answered, by the memory's words and
/// its expansions together. What clears it arrives marked `reached_by:
/// expansion` with the question's words the expansions supplied, after every
/// memory the question reached on its own and outside the answer core: an
/// expansion is a writer's guess a judge accepted, not something the memory
/// says. The citation is the memory's own text. An expansion never names an
/// anchor, since anchors are read from content alone.
///
/// Nothing happens where no candidate carries expansions, so those answers
/// stay byte for byte what they were.
pub(super) struct ExpansionRescue<'a> {
    pub(super) context: &'a AnswerRecallContext,
    pub(super) question: &'a str,
    pub(super) question_terms: &'a BTreeSet<String>,
    /// The focus a strict policy requires, and how many of its concepts.
    pub(super) strict: Option<&'a (BTreeSet<String>, usize)>,
}

impl ExpansionRescue<'_> {
    /// The rescued memories, strongest expansion match first, and what stays
    /// rejected.
    pub(super) fn rescue(
        &self,
        rejected: Vec<ReadCandidate>,
    ) -> (Vec<MemoryEvidence>, Vec<ReadCandidate>) {
        if self.question_terms.is_empty()
            || !rejected
                .iter()
                .any(|(_, terms)| terms.expansion_counts.length() > 0)
        {
            return (Vec::new(), rejected);
        }
        let field = LexicalField::build(
            rejected
                .iter()
                .map(|(_, terms)| &terms.expansion_counts)
                .filter(|counts| counts.length() > 0),
        );
        let mut asked = TermCounts::default();
        for term in self.question_terms {
            asked.insert(term.clone());
        }
        let weights = self
            .question_terms
            .iter()
            .map(|term| (term.clone(), 1.0))
            .collect::<BTreeMap<_, _>>();
        let floor = field.eligibility_floor(&asked);

        let mut expanded = Vec::new();
        let mut still_rejected = Vec::new();
        for (item, terms) in rejected {
            match self.supplied(&field, &weights, floor, &item, &terms) {
                Some((score, supplied)) => expanded.push((score, supplied, item, terms)),
                None => still_rejected.push((item, terms)),
            }
        }
        expanded.sort_by(|(left_score, _, left, _), (right_score, _, right, _)| {
            right_score
                .cmp(left_score)
                .then_with(|| stable_evidence_key(left).cmp(&stable_evidence_key(right)))
        });
        let mut rescued = Vec::new();
        for (_, supplied, item, terms) in expanded {
            if rescued.len() == MAX_EXPANDED_CANDIDATES {
                // Handed back untouched, with the terms it was read with.
                still_rejected.push((item, terms));
                continue;
            }
            rescued.push(self.mark(item, &supplied));
        }
        (rescued, still_rejected)
    }

    /// The expansion score and the question's search keys the expansions
    /// supplied, when they carry this candidate to the question.
    fn supplied(
        &self,
        field: &LexicalField,
        weights: &BTreeMap<String, f64>,
        floor: f64,
        item: &MemoryEvidence,
        terms: &AnswerCandidateTerms,
    ) -> Option<(i64, BTreeSet<String>)> {
        if terms.expansion_counts.length() == 0
            || self.context.temporal_state(item) != CandidateTemporalState::CurrentOrUnspecified
        {
            return None;
        }
        let score = field.score_weighted(weights, &terms.expansion_counts);
        if score <= 0.0 || score < floor * field.single_occurrence_factor(&terms.expansion_counts) {
            return None;
        }
        let expansion = terms
            .expansion_counts
            .terms()
            .cloned()
            .collect::<BTreeSet<_>>();
        let supplied = self
            .question_terms
            .iter()
            .filter(|term| expansion.contains(*term) && !terms.content.contains(*term))
            .cloned()
            .collect::<BTreeSet<_>>();
        // The expansions must be what made the difference.
        if supplied.is_empty() {
            return None;
        }
        if let Some((focus, required)) = self.strict {
            let reached = expansion
                .union(&terms.content)
                .cloned()
                .collect::<BTreeSet<_>>();
            if matching_term_count(focus, &reached) < *required {
                return None;
            }
        }
        Some((ranked_score(score), supplied))
    }

    /// Marks a rescued memory with its route and, in the reader's words, what
    /// the expansions supplied.
    fn mark(&self, item: MemoryEvidence, supplied: &BTreeSet<String>) -> MemoryEvidence {
        let morphology = &self.context.morphology;
        let words = informative_tokens(self.question)
            .filter(|token| supplied.contains(&search_key(token, morphology)))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");
        let mut item = mark_reached_by(item, REACHED_BY_EXPANSION);
        item.metadata.insert(EXPANSION_TERMS_KEY.to_string(), words);
        item
    }
}
