use super::super::embedded_errors::{kernel_error, mapping_error};
use super::read_telemetry::EmbeddedReadTelemetry;
use crate::projection::ask_from_response;
use crate::serving::adapters::tool_request_mapping::AskRequestMapper;
use crate::serving::ports::semantic_candidate_provider::SemanticCandidateProvider;
use crate::serving::{ToolError, tool_success_result};
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::recall_projection::project_ask_response;
use kmp_proto_mapping::v1beta1::{
    AskRetrievalContext, LexicalBridge, ask_query_from_proto, ask_response_from_result,
};
use serde_json::Value;
use std::sync::Arc;

/// Maps one Ask read, optional semantic ranking, and recall projection.
pub(crate) struct EmbeddedAskTool<'a> {
    service: &'a EmbeddedMemoryService,
    telemetry: EmbeddedReadTelemetry<'a>,
    bridge: &'a LexicalBridge,
    semantic: &'a Result<Option<Arc<dyn SemanticCandidateProvider>>, String>,
}

impl<'a> EmbeddedAskTool<'a> {
    pub(crate) fn new(
        service: &'a EmbeddedMemoryService,
        telemetry: EmbeddedReadTelemetry<'a>,
        bridge: &'a LexicalBridge,
        semantic: &'a Result<Option<Arc<dyn SemanticCandidateProvider>>, String>,
    ) -> Self {
        Self {
            service,
            telemetry,
            bridge,
            semantic,
        }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request =
            AskRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
        let query =
            ask_query_from_proto(request.clone()).map_err(|status| mapping_error(&status))?;
        let question = query.question.clone();
        let asked_as = query.asked_as.clone();
        let policy = query.answer_policy;
        let max_entries = query.max_entries;
        let temporal = query.temporal.clone();
        let about = query.about.clone();
        let result = self
            .service
            .ask(query)
            .await
            .map_err(kernel_error("ask", &about))?;
        self.telemetry
            .observe("kmp_ask", &result.bundle, &result.rendered.quality);
        let mut retrieval = AskRetrievalContext::from(result);
        let mut warning = None;
        match self.semantic {
            Ok(Some(provider)) => {
                let sources = retrieval
                    .semantic_sources(&temporal)
                    .map_err(|status| mapping_error(&status))?;
                let outcome = provider
                    .rank(
                        &question,
                        &sources,
                        arguments
                            .get("page")
                            .and_then(|p| p.get("cursor"))
                            .is_some(),
                    )
                    .await
                    .map_err(ToolError::invalid_argument)?;
                if let Some(ranking) = outcome.ranking {
                    retrieval = retrieval.with_semantic_candidates(ranking);
                }
                warning = outcome.warning;
            }
            Err(error) => warning = Some(format!("semantic retrieval disabled: {error}")),
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
        if let Some(warning) = warning {
            response.warnings.push(warning);
        }
        let response = project_ask_response(response, &request)
            .map_err(crate::projection::recall_error::projection)?;
        Ok(tool_success_result(ask_from_response(response)))
    }
}
