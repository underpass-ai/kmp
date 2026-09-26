use kmp_application::GetContextResult;
use kmp_domain::TemporalSelection;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use super::ranked_selection::RankedSelection;
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
    /// The lexical ranking a remote judge's pool was built from, which the
    /// answer reuses when it is asked with the same inputs.
    pub(super) ranked: Option<RankedSelection>,
    /// The anchored decision gate, unless the store opted out of it.
    pub(super) gate: Option<super::ask_gate::AskGate>,
}

impl From<GetContextResult> for AskRetrievalContext {
    fn from(result: GetContextResult) -> Self {
        Self {
            result,
            semantic: None,
            rerank: None,
            lexical_cache: None,
            ranked: None,
            gate: None,
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

    /// Decide with the store's anchored gate (on unless it opted out).
    pub fn with_gate(mut self, gate: super::ask_gate::AskGate) -> Self {
        self.gate = Some(gate);
        self
    }

    /// Decide with the default gate (`AskGate::STORE_DEFAULT`): the
    /// transport that reads no store configuration answers as a store that
    /// said nothing about it.
    pub fn with_default_gate(mut self) -> Self {
        self.gate = super::ask_gate::AskGate::STORE_DEFAULT;
        self
    }

    pub fn with_semantic_candidates(mut self, ranking: SemanticCandidateRanking) -> Self {
        self.semantic = Some(ranking);
        self
    }

    /// The passages a remote judge may reorder: the ranker's own order first,
    /// then admitted live entries it did not keep, so a paraphrase with no
    /// word in common can still be read. At most `limit`, unique by entry
    /// and exact text, and nothing the selection does not admit.
    ///
    /// The ranking read here is kept: the answer to the same question stands
    /// on it rather than ranking the pool a second time.
    pub fn rerank_pool(
        &mut self,
        question: &str,
        policy: kmp_application::MemoryAnswerPolicy,
        temporal: &TemporalSelection,
        bridge: &super::lexical_bridge::LexicalBridge,
        limit: usize,
    ) -> super::scalars::ProtoMappingResult<Vec<super::semantic_source::SemanticSource>> {
        use super::temporal_admission::TemporalAdmission;
        let lexical_identity =
            super::lexical_index_identity::LexicalIndexIdentity::read(&self.result, temporal);
        let admission = TemporalAdmission::read(&self.result.bundle, temporal)?;
        let bounded = admission.bound(&self.result.bundle);
        let lifecycle = super::responses::lifecycle_for(&bounded, &admission);
        let ranker =
            super::answer_ranker::AnswerEvidenceRanker::from_bundle_at(&bounded, bridge, lifecycle)
                .with_lexical_cache(self.lexical_cache.as_deref(), lexical_identity);
        let mut candidates = super::bundle_views::answer_evidence_from_bundle(&self.result.bundle)
            .into_iter()
            .filter(|item| admission.admits(item))
            .collect::<Vec<_>>();
        for evidence in &mut candidates {
            admission.bound_supports(evidence);
        }
        let ranked = RankedSelection::new(
            question,
            policy,
            temporal,
            bridge,
            ranker.rank(question, policy, candidates.clone()),
        );
        let pool = ranker.rerank_pool(ranked.ranked(), &candidates, limit);
        self.ranked = Some(ranked);
        Ok(pool)
    }

    pub fn with_rerank_candidates(mut self, ranking: RerankCandidateRanking) -> Self {
        self.rerank = Some(ranking);
        self
    }
}
