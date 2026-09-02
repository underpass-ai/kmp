use super::answer_candidate_terms::AnswerCandidateTerms;
use super::lexical_field::{LexicalField, ranked_score};
use super::morphology::Morphology;
use super::search_terms::informative_term_counts;
use super::term_counts::TermCounts;
use kmp_proto::v1beta1::MemoryEvidence;

/// Everything BM25 needs about one question and the candidates it is being
/// asked against.
pub(super) struct Lexicon {
    question: TermCounts,
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
        let floor = direct.eligibility_floor(&question);
        Self {
            question,
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
        let score = self.direct.score(&self.question, &terms.direct_counts);
        // Sharing nothing at all is its own answer, and no floor derived from
        // an empty overlap should be able to admit it.
        score > 0.0 && score >= self.floor
    }

    pub(super) fn content_score(&self, terms: &AnswerCandidateTerms) -> i64 {
        ranked_score(self.content.score(&self.question, &terms.content_counts))
    }

    pub(super) fn direct_score(&self, terms: &AnswerCandidateTerms) -> i64 {
        ranked_score(self.direct.score(&self.question, &terms.direct_counts))
    }
}
