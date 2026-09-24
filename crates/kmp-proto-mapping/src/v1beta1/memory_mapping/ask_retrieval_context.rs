use kmp_application::GetContextResult;
use kmp_domain::TemporalSelection;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use super::rerank_candidate_ranking::RerankCandidateRanking;
use super::semantic_candidate_ranking::SemanticCandidateRanking;

/// Context selected by the application, optionally accompanied by an external
/// semantic ranking. Existing transports use `From<GetContextResult>` and keep
/// their current deterministic retrieval behavior.
pub struct AskRetrievalContext {
    pub(super) result: GetContextResult,
    pub(super) semantic: Option<SemanticCandidateRanking>,
    pub(super) rerank: Option<RerankCandidateRanking>,
    pub(super) lexical_cache: Option<std::sync::Arc<super::lexical_index_cache::LexicalIndexCache>>,
}

impl From<GetContextResult> for AskRetrievalContext {
    fn from(result: GetContextResult) -> Self {
        Self {
            result,
            semantic: None,
            rerank: None,
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

    /// The passages a remote judge may reorder: the ranker's own order first,
    /// then admitted live entries it did not keep, so a paraphrase with no
    /// word in common can still be read. At most `limit`, unique by entry
    /// and exact text, and nothing the selection does not admit.
    pub fn rerank_pool(
        &self,
        question: &str,
        policy: kmp_application::MemoryAnswerPolicy,
        temporal: &TemporalSelection,
        bridge: &super::lexical_bridge::LexicalBridge,
        limit: usize,
    ) -> super::scalars::ProtoMappingResult<Vec<super::semantic_source::SemanticSource>> {
        use super::temporal_admission::TemporalAdmission;
        let admission = TemporalAdmission::read(&self.result.bundle, temporal)?;
        let bounded = admission.bound(&self.result.bundle);
        let lifecycle = super::responses::lifecycle_for(&bounded, &admission);
        let ranker =
            super::answer_ranker::AnswerEvidenceRanker::from_bundle_at(&bounded, bridge, lifecycle);
        let mut candidates = super::bundle_views::answer_evidence_from_bundle(&self.result.bundle)
            .into_iter()
            .filter(|item| admission.admits(item))
            .collect::<Vec<_>>();
        for evidence in &mut candidates {
            admission.bound_supports(evidence);
        }
        Ok(ranker.rerank_pool(question, policy, candidates, limit))
    }

    pub fn with_rerank_candidates(mut self, ranking: RerankCandidateRanking) -> Self {
        self.rerank = Some(ranking);
        self
    }
}
