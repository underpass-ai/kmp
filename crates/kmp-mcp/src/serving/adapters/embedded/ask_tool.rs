use super::super::embedded_errors::{kernel_error, mapping_error};
use super::super::judgement_reranker::JudgementReranker;
use super::frozen_recall_reads::FrozenRecallReads;
use super::read_telemetry::EmbeddedReadTelemetry;
use crate::projection::ask_from_response;
use crate::serving::adapters::tool_request_mapping::AskRequestMapper;
use crate::serving::frozen_recall::FrozenRecall;
use crate::serving::frozen_recall_key::FrozenRecallKey;
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
        }
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
                if !pool.is_empty() {
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
        Ok((response, revision))
    }
}
