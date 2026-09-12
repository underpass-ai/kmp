use super::super::embedded_errors::kernel_error;
use crate::projection::relabel_from_response;
use crate::serving::adapters::tool_request_mapping::RelabelRequestMapper;
use crate::serving::{ToolError, tool_success_result};
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::{relabel_command_from_proto, relabel_response_from_outcome};
use serde_json::Value;

/// Maps label changes and the existing application outcome.
pub(crate) struct EmbeddedRelabelTool<'a> {
    service: &'a EmbeddedMemoryService,
}

impl<'a> EmbeddedRelabelTool<'a> {
    pub(crate) fn new(service: &'a EmbeddedMemoryService) -> Self {
        Self { service }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request =
            RelabelRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
        let command = relabel_command_from_proto(request);
        let about = command.about.clone();
        let outcome = self
            .service
            .relabel(command)
            .await
            .map_err(kernel_error("relabel", &about))?;
        Ok(tool_success_result(relabel_from_response(
            relabel_response_from_outcome(outcome),
        )))
    }
}
