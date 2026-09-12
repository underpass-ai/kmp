use super::super::embedded_errors::{mapping_error, temporal_error};
use crate::projection::{enforce_temporal_output_budget, temporal_from_response};
use crate::serving::adapters::tool_request_mapping::NearRequestMapper;
use crate::serving::{ToolError, tool_success_result};
use kmp_domain::TemporalDirection;
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::{temporal_query_from_near_proto, temporal_response_from_result};
use serde_json::Value;

/// Maps Near with its around cursor and temporal output budget.
pub(crate) struct EmbeddedNearTool<'a> {
    service: &'a EmbeddedMemoryService,
}

impl<'a> EmbeddedNearTool<'a> {
    pub(crate) fn new(service: &'a EmbeddedMemoryService) -> Self {
        Self { service }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request =
            NearRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
        let requested_cursor = request.around.clone().unwrap_or_default();
        let query =
            temporal_query_from_near_proto(request).map_err(|status| mapping_error(&status))?;
        let about = query.about.clone();
        let result = self
            .service
            .temporal(query)
            .await
            .map_err(temporal_error("near", &about))?;
        // Structured temporal responses compute selection quality during mapping.
        // Do not render a discarded prompt merely to journal its token metrics.
        Ok(tool_success_result(enforce_temporal_output_budget(
            temporal_from_response(temporal_response_from_result(
                requested_cursor,
                TemporalDirection::Near,
                result,
            )),
            arguments,
        )?))
    }
}
