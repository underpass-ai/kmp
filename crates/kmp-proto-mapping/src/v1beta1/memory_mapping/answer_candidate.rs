use std::collections::BTreeSet;

use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate_terms::AnswerCandidateTerms;
use super::answer_recall_context::AnswerRecallContext;
use super::answer_selection::{
    intent_relation_matches, note_bridged_terms, note_summary_terms, relation_signal_total,
    stable_evidence_key,
};
use super::bridged_term::BridgedTerm;
use super::candidate_temporal_state::CandidateTemporalState;
use super::lexicon::Lexicon;
use super::morphology::Morphology;
use super::question_intent::QuestionIntent;
use super::ranking_focus::RankingFocus;
use super::relevance_key::RelevanceKey;
use super::search_terms::{
    informative_tokens, matching_term_count, matching_terms, query_requests_lifecycle, search_key,
};

pub(super) struct AnswerCandidate {
    pub(super) relevance: RelevanceKey,
    pub(super) searchable_terms: BTreeSet<String>,
    pub(super) stable_key: String,
    pub(super) item: MemoryEvidence,
    /// The question's search keys that reached this candidate through the
    /// writer's rendering and not through its text. Noted on the item, in
    /// the reader's words, once ranking is done.
    summary_supplied: BTreeSet<String>,
}

impl AnswerCandidate {
    /// Returns the candidate when the question reaches it directly, and hands
    /// the item back untouched, with the terms it was read with, when it does
    /// not, so a later pass can still rescue it without reading it again.
    pub(super) fn eligible(
        item: MemoryEvidence,
        terms: AnswerCandidateTerms,
        question_terms: &BTreeSet<String>,
        focus: RankingFocus<'_>,
        lexicon: &Lexicon,
        intent: &QuestionIntent,
        context: &AnswerRecallContext,
    ) -> Result<Self, Box<(MemoryEvidence, AnswerCandidateTerms)>> {
        if !lexicon.clears_floor(&terms) {
            return Err(Box::new((item, terms)));
        }
        // Both lifecycles end a claim's standing as current advice, and a
        // question that asks about history is asking for exactly them.
        if matches!(
            context.temporal_state(&item),
            CandidateTemporalState::Superseded | CandidateTemporalState::Expired
        ) && !query_requests_lifecycle(question_terms, &context.morphology)
        {
            return Err(Box::new((item, terms)));
        }

        let strict_focus = focus.strict;
        if !answers_focus(focus, lexicon, &terms) {
            return Err(Box::new((item, terms)));
        }
        // A citation that crossed a language says which words carried it.
        let bridged = lexicon.bridged_pairs(&terms);
        let item = if bridged.is_empty() {
            item
        } else {
            note_bridged_terms(item, &BridgedTerm::describe_all(bridged))
        };
        // A citation the writer's rendering carried will say which of the
        // question's words the rendering supplied and the text did not.
        let summary_supplied = summary_supplied_keys(question_terms, &terms);

        let relations = context.relationships_for(&item);
        let relevance = RelevanceKey {
            content_focus_matches: strict_focus
                .map(|(focus_terms, _)| matching_term_count(focus_terms, &terms.content))
                .unwrap_or_default(),
            content_score: lexicon.content_score(&terms),
            direct_score: lexicon.direct_score(&terms),
            claim_matches: matching_term_count(question_terms, &terms.claim),
            facet_kind_matches: focus
                .facet_kinds
                .zip(context.entry_kind(&item))
                .is_some_and(|(kinds, kind)| kinds.contains(kind))
                .into(),
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
            summary_supplied,
        })
    }

    /// The evidence as it is returned: with the words the rendering supplied
    /// noted on it, as the reader wrote them.
    ///
    /// Matching happened on the search key; reporting the key would answer
    /// with the kernel's vocabulary rather than the reader's, so the words
    /// are read back off the question.
    pub(super) fn into_item(self, question: &str, morphology: &Morphology) -> MemoryEvidence {
        if self.summary_supplied.is_empty() {
            return self.item;
        }
        let words = informative_tokens(question)
            .filter(|token| {
                self.summary_supplied
                    .contains(&search_key(token, morphology))
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");
        note_summary_terms(self.item, &words)
    }
}

/// Whether a candidate answers the focus a strict policy requires. A memory
/// in another language is about the same thing; the focus it must answer
/// counts a bridged concept as answered.
pub(super) fn answers_focus(
    focus: RankingFocus<'_>,
    lexicon: &Lexicon,
    terms: &AnswerCandidateTerms,
) -> bool {
    focus
        .strict
        .is_none_or(|(focus_terms, required_focus_matches)| {
            lexicon.focus_matches(focus_terms, terms) >= *required_focus_matches
        })
}

/// Whether the question reaches a candidate on its own words, whatever its
/// lifecycle: it clears the floor and answers the focus. `eligible` asks
/// this and then whether the candidate still stands.
pub(super) fn reached_by_the_question(
    focus: RankingFocus<'_>,
    lexicon: &Lexicon,
    terms: &AnswerCandidateTerms,
) -> bool {
    lexicon.clears_floor(terms) && answers_focus(focus, lexicon, terms)
}

/// The question's search keys that reached a candidate through its summary
/// and not through its text.
fn summary_supplied_keys(
    question_terms: &BTreeSet<String>,
    terms: &AnswerCandidateTerms,
) -> BTreeSet<String> {
    if terms.summary.is_empty() {
        return BTreeSet::new();
    }
    let through_text = matching_terms(question_terms, &terms.text);
    matching_terms(question_terms, &terms.summary)
        .difference(&through_text)
        .cloned()
        .collect()
}
