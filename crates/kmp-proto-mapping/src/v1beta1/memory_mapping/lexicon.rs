use super::answer_candidate_terms::AnswerCandidateTerms;
use super::association_index::AssociationIndex;
use super::lexical_field::{LexicalField, ranked_score};
use super::morphology::Morphology;
use super::search_terms::informative_term_counts;
use super::term_counts::TermCounts;
use kmp_proto::v1beta1::MemoryEvidence;
use std::collections::BTreeMap;

/// Everything BM25 needs about one question and the candidates it is being
/// asked against.
pub(super) struct Lexicon {
    question: TermCounts,
    /// The question as asked, plus what this memory says goes with it. The
    /// words the reader wrote weigh one; the store's own associations weigh a
    /// fraction.
    asked: BTreeMap<String, f64>,
    content: LexicalField,
    direct: LexicalField,
    floor: f64,
}

impl Lexicon {
    pub(super) fn build(
        question: &str,
        morphology: &Morphology,
        prepared: &[(MemoryEvidence, AnswerCandidateTerms)],
    ) -> Self {
        let question = informative_term_counts(question, morphology);
        let content = LexicalField::build(prepared.iter().map(|(_, terms)| &terms.content_counts));
        let direct = LexicalField::build(prepared.iter().map(|(_, terms)| &terms.direct_counts));
        // The bar stays what the reader asked for. Expansion may help a
        // candidate clear it; it may not lower it.
        let floor = direct.eligibility_floor(&question);
        let asked = AssociationIndex::build(prepared.iter().map(|(_, terms)| &terms.direct_counts))
            .expand(&question);
        Self {
            question,
            asked,
            content,
            direct,
            floor,
        }
    }

    /// Whether a candidate says enough about the question to be answering it.
    ///
    /// Measured on the raw score, not the quantized one, so a candidate is
    /// never refused by a rounding boundary.
    pub(super) fn clears_floor(&self, terms: &AnswerCandidateTerms) -> bool {
        let score = self
            .direct
            .score_weighted(&self.asked, &terms.direct_counts);
        // The bar is read in the candidate's own length, so a long entry that
        // says the one thing the question asked about is ranked low rather
        // than refused.
        let floor = self.floor * self.direct.single_occurrence_factor(&terms.direct_counts);
        // Sharing nothing at all is its own answer, and no floor derived from
        // an empty overlap should be able to admit it.
        score > 0.0 && score >= floor
    }

    /// Whether the store's own vocabulary carries the question to a candidate
    /// its words did not reach.
    ///
    /// The test is only that an expansion is what made the difference. The
    /// selectivity already happened where it can be measured and tuned: the
    /// index refuses a memory too small to have a pattern, a pair seen too few
    /// times to be one, an association no stronger than chance, and more than
    /// three neighbours for any word. Charging a second floor on top would
    /// charge the same care twice, and a candidate reached this way carries
    /// less than a whole concept by construction — which is exactly why it
    /// arrives marked and stays out of the answer.
    pub(super) fn is_associated(&self, terms: &AnswerCandidateTerms) -> bool {
        let expanded = self
            .direct
            .score_weighted(&self.asked, &terms.direct_counts);
        if expanded <= 0.0 {
            return false;
        }
        let asked_for = self
            .question
            .terms()
            .map(|term| (term.clone(), 1.0))
            .collect::<BTreeMap<_, _>>();
        expanded > self.direct.score_weighted(&asked_for, &terms.direct_counts)
    }

    pub(super) fn content_score(&self, terms: &AnswerCandidateTerms) -> i64 {
        ranked_score(
            self.content
                .score_weighted(&self.asked, &terms.content_counts),
        )
    }

    pub(super) fn direct_score(&self, terms: &AnswerCandidateTerms) -> i64 {
        ranked_score(
            self.direct
                .score_weighted(&self.asked, &terms.direct_counts),
        )
    }
}
