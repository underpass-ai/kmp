use kmp_application::GetContextResult;
use kmp_domain::TemporalSelection;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use super::semantic_candidate_ranking::SemanticCandidateRanking;

/// Context selected by the application, optionally accompanied by an external
/// semantic ranking. Existing transports use `From<GetContextResult>` and keep
/// their current deterministic retrieval behavior.
pub struct AskRetrievalContext {
    pub(super) result: GetContextResult,
    pub(super) semantic: Option<SemanticCandidateRanking>,
    pub(super) lexical_cache: Option<std::sync::Arc<super::lexical_index_cache::LexicalIndexCache>>,
}

impl From<GetContextResult> for AskRetrievalContext {
    fn from(result: GetContextResult) -> Self {
        Self {
            result,
            semantic: None,
            lexical_cache: None,
        }
    }
}

impl AskRetrievalContext {
    /// Reuse collection statistics only for an identical operation snapshot,
    /// selection and exact term counts. The returned evidence is always fresh.
    pub fn with_lexical_cache(
        mut self,
        cache: std::sync::Arc<super::lexical_index_cache::LexicalIndexCache>,
    ) -> Self {
        self.lexical_cache = Some(cache);
        self
    }

    /// The encoder sees only live text from the application's scoped result
    /// and the requested clock. Admission precedes semantic top-k selection.
    pub fn semantic_sources(
        &self,
        temporal: &TemporalSelection,
    ) -> super::scalars::ProtoMappingResult<Vec<super::semantic_source::SemanticSource>> {
        use super::answer_recall_context::AnswerRecallContext;
        use super::candidate_temporal_state::CandidateTemporalState;
        use super::memory_lifecycle::MemoryLifecycle;
        use super::semantic_source::SemanticSource;
        use super::temporal_admission::TemporalAdmission;
        let admission = TemporalAdmission::read(&self.result.bundle, temporal)?;
        let bounded = admission.bound(&self.result.bundle);
        let lifecycle = match admission.lifecycle_instant() {
            Some(instant) => MemoryLifecycle::read_at(&bounded, instant, admission.axis()),
            None => MemoryLifecycle::read(&bounded),
        };
        let context = AnswerRecallContext::from_bundle_with_lifecycle(&bounded, lifecycle);
        let mut sources = BTreeMap::new();
        for item in super::bundle_views::answer_evidence_from_bundle(&bounded) {
            if !admission.admits(&item)
                || context.temporal_state(&item) != CandidateTemporalState::CurrentOrUnspecified
            {
                continue;
            }
            let hash = format!("{:x}", Sha256::digest(item.text.as_bytes()));
            for entry_ref in item.supports {
                sources
                    .entry((entry_ref.clone(), hash.clone()))
                    .or_insert_with(|| SemanticSource {
                        entry_ref,
                        text: item.text.clone(),
                        text_sha256: hash.clone(),
                    });
            }
        }
        Ok(sources.into_values().collect())
    }

    pub fn with_semantic_candidates(mut self, ranking: SemanticCandidateRanking) -> Self {
        self.semantic = Some(ranking);
        self
    }
}
