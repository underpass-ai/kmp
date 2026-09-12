use super::super::embedded_errors::{mapping_error, temporal_error};
use crate::projection::{enforce_temporal_output_budget, temporal_from_response};
use crate::serving::adapters::tool_request_mapping::TemporalMoveRequestMapper;
use crate::serving::{ToolError, tool_success_result};
use kmp_domain::TemporalDirection;
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::{temporal_query_from_move_proto, temporal_response_from_result};
use serde_json::Value;

/// Maps Goto, Rewind, or Forward through the existing temporal use case.
pub(crate) struct EmbeddedTemporalMoveTool<'a> {
    service: &'a EmbeddedMemoryService,
    direction: TemporalDirection,
    direction_name: &'static str,
}

impl<'a> EmbeddedTemporalMoveTool<'a> {
    pub(crate) fn new(
        service: &'a EmbeddedMemoryService,
        direction: TemporalDirection,
        direction_name: &'static str,
    ) -> Self {
        Self {
            service,
            direction,
            direction_name,
        }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request = TemporalMoveRequestMapper::from_arguments(arguments, self.direction_name)
            .map_err(ToolError::invalid_argument)?;
        let requested_cursor = request.cursor.clone().unwrap_or_default();
        let query = temporal_query_from_move_proto(request, self.direction)
            .map_err(|status| mapping_error(&status))?;
        let about = query.about.clone();
        let result = self
            .service
            .temporal(query)
            .await
            .map_err(temporal_error(self.direction_name, &about))?;
        // Structured temporal responses compute selection quality during mapping.
        // Do not render a discarded prompt merely to journal its token metrics.
        Ok(tool_success_result(enforce_temporal_output_budget(
            temporal_from_response(temporal_response_from_result(
                requested_cursor,
                self.direction,
                result,
            )),
            arguments,
        )?))
    }
}
