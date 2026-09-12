use super::super::embedded_errors::{kernel_error, mapping_error};
use super::read_telemetry::EmbeddedReadTelemetry;
use crate::projection::wake_from_response;
use crate::serving::adapters::tool_request_mapping::WakeRequestMapper;
use crate::serving::{ToolError, tool_success_result};
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::recall_projection::project_wake_response;
use kmp_proto_mapping::v1beta1::{wake_query_from_proto, wake_response_from_result};
use serde_json::Value;

/// Maps one wake read and projects its observed result.
pub(crate) struct EmbeddedWakeTool<'a> {
    service: &'a EmbeddedMemoryService,
    telemetry: EmbeddedReadTelemetry<'a>,
}

impl<'a> EmbeddedWakeTool<'a> {
    pub(crate) fn new(
        service: &'a EmbeddedMemoryService,
        telemetry: EmbeddedReadTelemetry<'a>,
    ) -> Self {
        Self { service, telemetry }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request =
            WakeRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
        let query =
            wake_query_from_proto(request.clone()).map_err(|status| mapping_error(&status))?;
        let intent = query.intent.clone();
        let max_entries = query.max_entries;
        let temporal = query.temporal.clone();
        let about = query.about.clone();
        let result = self
            .service
            .wake(query)
            .await
            .map_err(kernel_error("wake", &about))?;
        self.telemetry
            .observe("kmp_wake", &result.bundle, &result.rendered.quality);
        let response = project_wake_response(
            wake_response_from_result(&intent, max_entries, result, &temporal)
                .map_err(|status| mapping_error(&status))?,
            &request,
        )
        .map_err(crate::projection::recall_error::projection)?;
        Ok(tool_success_result(wake_from_response(response)))
    }
}
