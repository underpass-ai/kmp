use super::super::embedded_errors::{kernel_error, mapping_error};
use crate::projection::{enforce_inspect_output_budget, inspect_from_response};
use crate::serving::adapters::tool_request_mapping::InspectRequestMapper;
use crate::serving::{ToolError, tool_success_result};
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::{inspect_query_from_proto, inspect_response_from_result};
use serde_json::Value;

/// Maps one Inspect read and its output budget.
pub(crate) struct EmbeddedInspectTool<'a> {
    service: &'a EmbeddedMemoryService,
}

impl<'a> EmbeddedInspectTool<'a> {
    pub(crate) fn new(service: &'a EmbeddedMemoryService) -> Self {
        Self { service }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request =
            InspectRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
        let query = inspect_query_from_proto(request).map_err(|status| mapping_error(&status))?;
        let ref_id = query.ref_id.clone();
        let result = self
            .service
            .inspect(query)
            .await
            .map_err(kernel_error("inspect", &ref_id))?;
        Ok(tool_success_result(enforce_inspect_output_budget(
            inspect_from_response(inspect_response_from_result(result)),
            arguments,
        )?))
    }
}
