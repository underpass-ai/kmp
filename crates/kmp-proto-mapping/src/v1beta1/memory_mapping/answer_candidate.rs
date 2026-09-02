use std::collections::BTreeSet;

use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate_terms::AnswerCandidateTerms;
use super::answer_recall_context::AnswerRecallContext;
use super::answer_selection::{
    intent_relation_matches, relation_signal_total, stable_evidence_key,
};
use super::candidate_temporal_state::CandidateTemporalState;
use super::lexicon::Lexicon;
use super::question_intent::QuestionIntent;
use super::relevance_key::RelevanceKey;
use super::search_terms::{matching_term_count, query_requests_lifecycle};

pub(super) struct AnswerCandidate {
    pub(super) relevance: RelevanceKey,
    pub(super) searchable_terms: BTreeSet<String>,
    pub(super) stable_key: String,
    pub(super) item: MemoryEvidence,
}

impl AnswerCandidate {
    /// Returns the candidate when the question reaches it directly, and hands
    /// the item back untouched when it does not, so a later pass can still
    /// rescue it through the graph.
    pub(super) fn eligible(
        item: MemoryEvidence,
        terms: AnswerCandidateTerms,
        question_terms: &BTreeSet<String>,
        strict_focus: Option<&(BTreeSet<String>, usize)>,
        lexicon: &Lexicon,
        intent: &QuestionIntent,
        context: &AnswerRecallContext,
    ) -> Result<Self, Box<MemoryEvidence>> {
        if !lexicon.clears_floor(&terms) {
            return Err(Box::new(item));
        }
        // Both lifecycles end a claim's standing as current advice, and a
        // question that asks about history is asking for exactly them.
        if matches!(
            context.temporal_state(&item),
            CandidateTemporalState::Superseded | CandidateTemporalState::Expired
        ) && !query_requests_lifecycle(question_terms, &context.morphology)
        {
            return Err(Box::new(item));
        }

        let answers_requested_focus =
            strict_focus.is_none_or(|(focus_terms, required_focus_matches)| {
                matching_term_count(focus_terms, &terms.searchable) >= *required_focus_matches
            });
        if !answers_requested_focus {
            return Err(Box::new(item));
        }

        let relations = context.relationships_for(&item);
        let relevance = RelevanceKey {
            content_focus_matches: strict_focus
                .map(|(focus_terms, _)| matching_term_count(focus_terms, &terms.content))
                .unwrap_or_default(),
            content_score: lexicon.content_score(&terms),
            direct_score: lexicon.direct_score(&terms),
            claim_matches: matching_term_count(question_terms, &terms.claim),
            intent_relation_matches: intent_relation_matches(intent, &relations),
            relation_why_matches: matching_term_count(question_terms, &terms.relation_why),
            relation_matches: matching_term_count(question_terms, &terms.relation),
            relation_signal: relation_signal_total(question_terms, &relations),
            total_matches: matching_term_count(question_terms, &terms.searchable),
            recency_rank: context.recency_rank(&item),
        };
        Ok(Self {
            relevance,
            searchable_terms: terms.searchable,
            stable_key: stable_evidence_key(&item),
            item,
        })
    }
}
