use kmp_application::GetContextResult;

use super::semantic_candidate_ranking::SemanticCandidateRanking;

/// Context selected by the application, optionally accompanied by an external
/// semantic ranking. Existing transports use `From<GetContextResult>` and keep
/// their current deterministic retrieval behavior.
pub struct AskRetrievalContext {
    pub(super) result: GetContextResult,
    pub(super) semantic: Option<SemanticCandidateRanking>,
}

impl From<GetContextResult> for AskRetrievalContext {
    fn from(result: GetContextResult) -> Self {
        Self {
            result,
            semantic: None,
        }
    }
}

impl AskRetrievalContext {
    pub fn with_semantic_candidates(mut self, ranking: SemanticCandidateRanking) -> Self {
        self.semantic = Some(ranking);
        self
    }
}
