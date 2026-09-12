use super::super::embedded_errors::{kernel_error, mapping_error};
use super::read_telemetry::EmbeddedReadTelemetry;
use crate::projection::relate_from_response;
use crate::projection::relation_page_budget::RelationPageBudget;
use crate::serving::adapters::tool_request_mapping::RelateRequestMapper;
use crate::serving::{ToolError, tool_success_result};
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::{
    LexicalBridge, relate_query_from_proto, relate_response_from_result,
};
use serde_json::Value;

/// Maps a Relate read and budgets its fingerprint-bound response.
pub(crate) struct EmbeddedRelateTool<'a> {
    service: &'a EmbeddedMemoryService,
    telemetry: EmbeddedReadTelemetry<'a>,
    bridge: &'a LexicalBridge,
}

impl<'a> EmbeddedRelateTool<'a> {
    pub(crate) fn new(
        service: &'a EmbeddedMemoryService,
        telemetry: EmbeddedReadTelemetry<'a>,
        bridge: &'a LexicalBridge,
    ) -> Self {
        Self {
            service,
            telemetry,
            bridge,
        }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request =
            RelateRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
        let query = relate_query_from_proto(request).map_err(|status| mapping_error(&status))?;
        let about = query.about.clone();
        let result = self
            .service
            .relate(query.clone())
            .await
            .map_err(kernel_error("relate", &about))?;
        self.telemetry
            .observe("kmp_relate", &result.bundle, &result.rendered.quality);
        let response = relate_response_from_result(result, &query, self.bridge)
            .map_err(|status| mapping_error(&status))?;
        let fingerprint = response.selection_fingerprint.clone();
        Ok(tool_success_result(RelationPageBudget::Relate.apply(
            relate_from_response(response),
            arguments,
            &fingerprint,
        )?))
    }
}
