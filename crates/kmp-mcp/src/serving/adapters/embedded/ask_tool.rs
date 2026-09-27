use super::super::doubt_band_judge::DoubtBandJudge;
use super::super::embedded_errors::{kernel_error, mapping_error};
use super::super::judgement_reranker::JudgementReranker;
use super::super::lexical_index::lexical_sidecar::{
    LEXICAL_ASK_DEPTH, LexicalSidecar, ShadowScope,
};
use super::frozen_recall_reads::FrozenRecallReads;
use super::read_telemetry::EmbeddedReadTelemetry;
use crate::projection::ask_from_response;
use crate::serving::adapters::tool_request_mapping::AskRequestMapper;
use crate::serving::frozen_recall::FrozenRecall;
use crate::serving::frozen_recall_key::FrozenRecallKey;
use crate::serving::lexical_index_mode::LexicalIndexMode;
use crate::serving::ports::semantic_candidate_provider::SemanticCandidateProvider;
use crate::serving::{ToolError, tool_success_result};
use kmp_application::RenderDemand;
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto::v1beta1::AskResponse;
use kmp_proto_mapping::v1beta1::recall_projection::{project_rendered_ask, render_ask};
use kmp_proto_mapping::v1beta1::{
    AskGate, AskRetrievalContext, LexicalBridge, ask_query_from_proto, ask_response_from_result,
};
use serde_json::Value;
use std::sync::Arc;

/// Maps one Ask read, optional semantic ranking, and recall projection.
pub(crate) struct EmbeddedAskTool<'a> {
    service: &'a EmbeddedMemoryService,
    telemetry: EmbeddedReadTelemetry<'a>,
    bridge: &'a LexicalBridge,
    semantic: &'a Result<Option<Arc<dyn SemanticCandidateProvider>>, String>,
    rerank: &'a Result<Option<Arc<JudgementReranker>>, String>,
    lexical_cache: &'a Arc<kmp_proto_mapping::v1beta1::LexicalIndexCache>,
    frozen: FrozenRecallReads<'a>,
    gate: Option<AskGate>,
    doubt_band: Option<&'a Result<Option<Arc<DoubtBandJudge>>, String>>,
    lexical: Option<(&'a LexicalSidecar, &'a kmp_embedded::EmbeddedKernelStore)>,
}

impl<'a> EmbeddedAskTool<'a> {
    pub(crate) fn new(
        service: &'a EmbeddedMemoryService,
        telemetry: EmbeddedReadTelemetry<'a>,
        bridge: &'a LexicalBridge,
        semantic: &'a Result<Option<Arc<dyn SemanticCandidateProvider>>, String>,
        rerank: &'a Result<Option<Arc<JudgementReranker>>, String>,
        lexical_cache: &'a Arc<kmp_proto_mapping::v1beta1::LexicalIndexCache>,
        frozen: FrozenRecallReads<'a>,
    ) -> Self {
        Self {
            service,
            telemetry,
            bridge,
            semantic,
            rerank,
            lexical_cache,
            frozen,
            gate: None,
            doubt_band: None,
            lexical: None,
        }
    }

    /// Follows the lexical sidecar before the read and compares it with the
    /// ranking after it (shadow mode); the answer never depends on it.
    pub(crate) fn with_lexical_sidecar(
        mut self,
        sidecar: &'a LexicalSidecar,
        store: &'a kmp_embedded::EmbeddedKernelStore,
    ) -> Self {
        self.lexical = Some((sidecar, store));
        self
    }

    /// Ask the store's doubt band judge (`ask-judge.json`), if it opted in.
    pub(crate) fn with_doubt_band(
        mut self,
        doubt_band: &'a Result<Option<Arc<DoubtBandJudge>>, String>,
    ) -> Self {
        self.doubt_band = Some(doubt_band);
        self
    }

    /// Decide with the anchored gate the store opted into, if any.
    pub(crate) fn with_gate(mut self, gate: Option<AskGate>) -> Self {
        self.gate = gate;
        self
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request =
            AskRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
        let query =
            ask_query_from_proto(request.clone()).map_err(|status| mapping_error(&status))?;
        let key = FrozenRecallKey::Ask(query.clone());
        // A continuation of an unchanged store cuts its page from the first
        // page's read and render: same ranking, same remote verdicts, same
        // bytes.
        let (response, rendered, revision) = match self.frozen.thaw(&key, arguments).await {
            Some(FrozenRecall::Ask { response, rendered }) => (*response, rendered, None),
            _ => {
                let (response, revision) = self.read(query, arguments).await?;
                let rendered = render_ask(&response);
                (response, rendered, revision)
            }
        };
        let kept = revision
            .is_some()
            .then(|| (response.clone(), rendered.clone()));
        let response = project_rendered_ask(response, rendered, &request)
            .map_err(crate::projection::recall_error::projection)?;
        if let Some((kept, rendered)) = kept {
            self.frozen.freeze(
                key,
                revision,
                FrozenRecall::Ask {
                    response: Box::new(kept),
                    rendered,
                },
                response.projection.as_ref(),
            );
        }
        Ok(tool_success_result(ask_from_response(response)))
    }

    /// The kernel read, its ranking and the remote channels' verdicts,
    /// before any page is cut; with the revision it was read at.
    async fn read(
        &self,
        query: kmp_application::memory::AskMemoryQuery,
        arguments: &Value,
    ) -> Result<(AskResponse, Option<kmp_domain::GraphReadRevision>), ToolError> {
        let question = query.question.clone();
        let asked_as = query.asked_as.clone();
        let policy = query.answer_policy;
        let max_entries = query.max_entries;
        let temporal = query.temporal.clone();
        let about = query.about.clone();
        // The sidecar indexes what an ask with no depth, dimensions or clock
        // of its own reads.
        let read = ShadowScope::of(&query, LEXICAL_ASK_DEPTH);
        let followed = match self.lexical {
            Some((sidecar, store)) => sidecar.catch_up(store, Some(&about)).await,
            None => None,
        };
        let witness = self.lexical.and_then(|(sidecar, _)| sidecar.witness());
        // An ask the lexical index can hold is answered from the candidates
        // its postings reach (DESIGN L6, P13): one about at the frontier
        // with no dimensions at the indexed depth (or deeper, while nothing
        // lies past it), and no channel that reads
        // the whole admitted pool (semantic retrieval, re-ranking, the doubt
        // band). The bridge reads the whole about's vocabulary from the index.
        let indexed = match self.lexical {
            Some((sidecar, store))
                if sidecar.mode().reads_postings()
                    && matches!(read, ShadowScope::Indexed { .. })
                    && matches!(self.semantic, Ok(None))
                    && matches!(self.rerank, Ok(None))
                    && !matches!(self.doubt_band, Some(Ok(Some(_)) | Err(_))) =>
            {
                let started = std::time::Instant::now();
                let read = sidecar
                    .indexed_read(
                        store,
                        self.service,
                        &query,
                        followed.as_ref(),
                        self.bridge,
                        read.deeper(),
                    )
                    .await;
                Some((sidecar.mode(), read, started))
            }
            _ => None,
        };
        let indexed = match indexed {
            Some((mode, Ok(Ok(read)), started)) => {
                let revision = read.result.read_revision.clone();
                let (plan_us, parts_us, documents) = (read.plan_us, read.parts_us, read.documents);
                let mut retrieval = AskRetrievalContext::from(read.result);
                if let Some(gate) = self.gate {
                    retrieval = retrieval.with_gate(gate);
                }
                let answered = ask_response_from_result(
                    &question,
                    asked_as.as_deref(),
                    policy,
                    max_entries,
                    retrieval.with_indexed(read.indexed),
                    self.bridge,
                    &temporal,
                )
                .map_err(|status| mapping_error(&status))?;
                let elapsed_us = started.elapsed().as_micros() as u64;
                if mode == LexicalIndexMode::On {
                    tracing::debug!(
                        target: "kmp_mcp::lexical_index",
                        event = "kmp_lexical_answer",
                        answered = true,
                        candidates = read.candidates,
                        documents,
                        plan_us,
                        parts_us,
                        elapsed_us,
                        "ask answered from the lexical index"
                    );
                    return Ok((answered, revision));
                }
                Some((
                    answered,
                    read.candidates,
                    documents,
                    elapsed_us,
                    plan_us,
                    parts_us,
                ))
            }
            Some((_, Ok(Err(why)), _)) => {
                tracing::debug!(
                    target: "kmp_mcp::lexical_index",
                    event = "kmp_lexical_answer",
                    answered = false,
                    reason = why,
                    "ask read the about"
                );
                None
            }
            Some((_, Err(error), _)) => {
                tracing::warn!(target: "kmp_mcp::lexical_index", %error, "lexical index could not answer; the ask reads the about");
                None
            }
            None => None,
        };
        let started = std::time::Instant::now();
        let result = self
            .service
            .ask_on_demand(query, self.telemetry.render_demand(RenderDemand::Skip))
            .await
            .map_err(kernel_error("ask", &about))?;
        self.telemetry
            .trace_timing("kmp_ask", result.timing.as_ref());
        self.telemetry
            .observe("kmp_ask", &result.bundle, &result.rendered);
        let revision = result.read_revision.clone();
        let mut retrieval =
            AskRetrievalContext::from(result).with_lexical_cache(Arc::clone(self.lexical_cache));
        if let Some(gate) = self.gate {
            retrieval = retrieval.with_gate(gate);
        }
        if let Some(witness) = &witness {
            retrieval = retrieval.with_lexical_witness(Arc::clone(witness));
        }
        let mut warnings = Vec::new();
        let continuation = arguments
            .get("page")
            .and_then(|p| p.get("cursor"))
            .is_some();
        match self.semantic {
            Ok(Some(provider)) => {
                let sources = retrieval
                    .semantic_sources(&temporal)
                    .map_err(|status| mapping_error(&status))?;
                let outcome = provider
                    .rank(&question, &sources, continuation)
                    .await
                    .map_err(ToolError::invalid_argument)?;
                if let Some(ranking) = outcome.ranking {
                    retrieval = retrieval.with_semantic_candidates(ranking);
                }
                warnings.extend(outcome.warning);
            }
            Err(error) => warnings.push(format!("semantic retrieval disabled: {error}")),
            Ok(None) => {}
        }
        // Remote re-ranking reads the admitted pool in the ranker's order and
        // then what the ranker left out; it reorders proof, never the answer.
        match self.rerank {
            Ok(Some(reranker)) => {
                let pool = retrieval
                    .rerank_pool(
                        &question,
                        policy,
                        &temporal,
                        self.bridge,
                        reranker.pool_size(),
                    )
                    .map_err(|status| mapping_error(&status))?;
                // A lead the text settles is not sent to the judge.
                if !pool.is_empty() && !reranker.is_settled(retrieval.lexical_margin()) {
                    let outcome = reranker
                        .rank(&question, &pool, continuation)
                        .await
                        .map_err(ToolError::invalid_argument)?;
                    if let Some(ranking) = outcome.ranking {
                        retrieval = retrieval.with_rerank_candidates(ranking);
                    }
                    warnings.extend(outcome.warning);
                }
            }
            Err(error) => warnings.push(format!("evidence rerank disabled: {error}")),
            Ok(None) => {}
        }
        // The doubt band asks a judge only on a first page, and only when
        // the deterministic reading settled in doubt; its verdicts act on
        // the core, never on anything the selection did not admit.
        match self.doubt_band {
            Some(Ok(Some(judge))) if !continuation => {
                let band = retrieval
                    .doubt_band(
                        &question,
                        policy,
                        &temporal,
                        self.bridge,
                        judge.margin_tenths(),
                    )
                    .map_err(|status| mapping_error(&status))?;
                if let Some(band) = band {
                    let outcome = judge.judge(&question, &band).await;
                    if let Some(verdicts) = outcome.verdicts {
                        retrieval = retrieval.with_doubt_verdicts(verdicts);
                    }
                    warnings.extend(outcome.warning);
                }
            }
            Some(Err(error)) => warnings.push(format!("doubt band disabled: {error}")),
            _ => {}
        }
        let mut response = ask_response_from_result(
            &question,
            asked_as.as_deref(),
            policy,
            max_entries,
            retrieval,
            self.bridge,
            &temporal,
        )
        .map_err(|status| mapping_error(&status))?;
        response.warnings.extend(warnings);
        // Verify: the index's answer beside the one the about gave.
        if let Some((answered, candidates, documents, elapsed_us, plan_us, parts_us)) = indexed {
            tracing::info!(
                target: "kmp_mcp::lexical_index",
                event = "kmp_lexical_verify",
                equal = answered == response,
                candidates,
                documents,
                plan_us,
                parts_us,
                index_elapsed_us = elapsed_us,
                about_elapsed_us = started.elapsed().as_micros() as u64,
                "lexical index answer compared"
            );
        }
        if let (Some((sidecar, store)), Some(witness)) = (self.lexical, witness) {
            sidecar
                .shadow(store, &about, witness.take(), read, followed)
                .await;
        }
        Ok((response, revision))
    }
}
