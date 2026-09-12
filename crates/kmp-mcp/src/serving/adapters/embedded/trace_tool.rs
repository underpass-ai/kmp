use super::super::embedded_errors::{kernel_error, mapping_error};
use super::read_telemetry::EmbeddedReadTelemetry;
use crate::projection::relation_page_budget::RelationPageBudget;
use crate::projection::trace_from_response;
use crate::serving::adapters::tool_request_mapping::TraceRequestMapper;
use crate::serving::{ToolError, tool_success_result};
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::{trace_query_from_proto, trace_response_from_result};
use serde_json::Value;

/// Maps Trace in seek, target-search, or legacy mode, in that order.
pub(crate) struct EmbeddedTraceTool<'a> {
    service: &'a EmbeddedMemoryService,
    telemetry: EmbeddedReadTelemetry<'a>,
}

impl<'a> EmbeddedTraceTool<'a> {
    pub(crate) fn new(
        service: &'a EmbeddedMemoryService,
        telemetry: EmbeddedReadTelemetry<'a>,
    ) -> Self {
        Self { service, telemetry }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request =
            TraceRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
        if let Some(query) = kmp_proto_mapping::v1beta1::evidence_seek_request_from_proto(&request)
            .map_err(|s| mapping_error(&s))?
        {
            let page = trace_query_from_proto(request.clone())
                .map_err(|s| mapping_error(&s))?
                .page;
            let result = self
                .service
                .evidence_paths(query.clone())
                .await
                .map_err(kernel_error("trace", "evidence seek"))?;
            let seek = request
                .search
                .as_ref()
                .and_then(|s| s.seek.as_ref())
                .expect("compiled seek");
            let response = kmp_proto_mapping::v1beta1::evidence_seek_response_from_result(
                result, &query, seek, page,
            );
            let fingerprint = response.selection_fingerprint.clone();
            return Ok(tool_success_result(RelationPageBudget::Trace.apply(
                trace_from_response(response),
                arguments,
                &fingerprint,
            )?));
        }
        if let Some(search) = kmp_proto_mapping::v1beta1::trace_search_request_from_proto(&request)
            .map_err(|s| mapping_error(&s))?
        {
            let direction = search.direction;
            let page = trace_query_from_proto(request)
                .map_err(|s| mapping_error(&s))?
                .page;
            let result = self
                .service
                .trace_search(search)
                .await
                .map_err(kernel_error("trace", "bounded search"))?;
            let response = kmp_proto_mapping::v1beta1::trace_search_response_from_result(
                result, direction, page,
            );
            let fingerprint = response.selection_fingerprint.clone();
            return Ok(tool_success_result(RelationPageBudget::Trace.apply(
                trace_from_response(response),
                arguments,
                &fingerprint,
            )?));
        }
        let query = trace_query_from_proto(request).map_err(|status| mapping_error(&status))?;
        let page = query.page.clone();
        let from = query.from.clone();
        let result = self
            .service
            .trace(query)
            .await
            .map_err(kernel_error("trace", &from))?;
        self.telemetry
            .observe("kmp_trace", &result.path_bundle, &result.rendered.quality);
        let response = trace_response_from_result(result, page);
        let fingerprint = response.selection_fingerprint.clone();
        Ok(tool_success_result(RelationPageBudget::Trace.apply(
            trace_from_response(response),
            arguments,
            &fingerprint,
        )?))
    }
}
