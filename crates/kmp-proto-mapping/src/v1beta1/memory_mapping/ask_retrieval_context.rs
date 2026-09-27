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
    /// How decisively the lexical ranking a judge's pool was built from
    /// leads, once that pool was built.
    pub(super) margin: Option<super::lexical_margin::LexicalMargin>,
    /// The anchored reading a doubt band took before asking its judge,
    /// which the answer reuses when no verdict came back.
    pub(super) decided: Option<super::decided_selection::DecidedSelection>,
    /// A doubt band judge's verdicts, applied to the core by the answer.
    pub(super) doubt: Option<super::doubt_verdicts::DoubtVerdicts>,
    /// Where the answer's ranker records what it measured, for the lexical
    /// sidecar's shadow comparison.
    pub(super) witness: Option<std::sync::Arc<super::lexical_shadow_witness::LexicalShadowWitness>>,
    /// The whole about as the lexical index holds it, when `result` holds
    /// only the candidates its postings reached (DESIGN L6, P13).
    pub(super) indexed: Option<super::indexed_ask::IndexedAsk>,
    /// The terms the doubt band's reading read the candidates with, which
    /// the answer's reading of the same candidates takes back (P10).
    pub(super) prepared: super::prepared_terms_cache::PreparedTermsCache,
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
            margin: None,
            decided: None,
            doubt: None,
            witness: None,
            indexed: None,
            prepared: Default::default(),
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

    /// Records what the answer's ranker measures in `witness`, for the
    /// lexical sidecar's shadow comparison. Never changes the answer.
    pub fn with_lexical_witness(
        mut self,
        witness: std::sync::Arc<super::lexical_shadow_witness::LexicalShadowWitness>,
    ) -> Self {
        self.witness = Some(witness);
        self
    }

    /// Ranks the candidates `result` holds against the whole about the
    /// lexical index describes (DESIGN L6, P13). `result` must hold every
    /// candidate the postings of the question's words reach, and the
    /// neighbourhood the ranker's rescues walk from them.
    pub fn with_indexed(mut self, indexed: super::indexed_ask::IndexedAsk) -> Self {
        self.indexed = Some(indexed);
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
                .with_lexical_cache(self.lexical_cache.as_deref(), lexical_identity)
                .with_lexical_witness(self.witness.as_deref());
        let mut candidates = super::bundle_views::answer_evidence_from_bundle(&self.result.bundle)
            .into_iter()
            .filter(|item| admission.admits(item))
            .collect::<Vec<_>>();
        for evidence in &mut candidates {
            admission.bound_supports(evidence);
        }
        let (ranking, scores) = ranker.rank_scored(question, policy, candidates.clone());
        let ranked = RankedSelection::new(question, policy, temporal, bridge, ranking);
        let pool = ranker.rerank_pool(ranked.ranked(), &candidates, limit);
        let core = ranked
            .ranked()
            .iter()
            .filter(|item| !super::answer_selection::was_reached_indirectly(item))
            .take(super::answer_ranker::ANSWER_CORE_LIMIT)
            .cloned()
            .collect::<Vec<_>>();
        self.margin = Some(scores.lead().with_high_confidence(
            ranker.confidence(question, &core) == kmp_proto::v1beta1::MemoryConfidence::High,
        ));
        self.ranked = Some(ranked);
        Ok(pool)
    }

    /// How decisively the lexical ranking leads, once [`Self::rerank_pool`]
    /// read it; `None` before.
    pub fn lexical_margin(&self) -> Option<super::lexical_margin::LexicalMargin> {
        self.margin
    }

    pub fn with_rerank_candidates(mut self, ranking: RerankCandidateRanking) -> Self {
        self.rerank = Some(ranking);
        self
    }

    /// Whether this ask falls in the doubt band (DESIGN L4 4f), and the
    /// passages a judge would be asked about. `None` without the anchored
    /// gate, under `best_effort`, when a required anchor is absent, and when
    /// the deterministic reading settled clearly: answered with a first
    /// citation leading the second by at least `margin_below` tenths of a
    /// BM25 point, or UNKNOWN with nothing a `best_effort` reading would cite.
    ///
    /// The reading taken here is kept: an answer given without verdicts
    /// stands on it instead of reading the question a second time.
    pub fn doubt_band(
        &mut self,
        question: &str,
        policy: kmp_application::MemoryAnswerPolicy,
        temporal: &TemporalSelection,
        bridge: &super::lexical_bridge::LexicalBridge,
        margin_below: i64,
    ) -> super::scalars::ProtoMappingResult<Option<super::doubt_band::DoubtBand>> {
        let (band, decided, ranked) = super::doubt_band_reading::read_doubt_band(
            self,
            question,
            policy,
            temporal,
            bridge,
            margin_below,
        )?;
        if decided.is_some() {
            self.decided = decided;
        }
        if ranked.is_some() {
            self.ranked = ranked;
        }
        Ok(band)
    }

    /// The answer applies a doubt band judge's verdicts to its core.
    pub fn with_doubt_verdicts(mut self, verdicts: super::doubt_verdicts::DoubtVerdicts) -> Self {
        self.doubt = Some(verdicts);
        self
    }
}
