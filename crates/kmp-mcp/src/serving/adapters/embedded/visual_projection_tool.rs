use super::super::embedded_errors::{kernel_error, mapping_error};
use crate::projection::visual_projection_from_response;
use crate::serving::adapters::tool_request_mapping::VisualProjectionRequestMapper;
use crate::serving::{ToolError, app_data_success_result};
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::{
    visual_projection_query_from_proto, visual_projection_response_from_result,
};
use serde_json::Value;

/// Maps a visual projection read into the app-data envelope.
pub(crate) struct EmbeddedVisualProjectionTool<'a> {
    service: &'a EmbeddedMemoryService,
}

impl<'a> EmbeddedVisualProjectionTool<'a> {
    pub(crate) fn new(service: &'a EmbeddedMemoryService) -> Self {
        Self { service }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request = VisualProjectionRequestMapper::from_arguments(arguments)
            .map_err(ToolError::invalid_argument)?;
        let about = request.about.clone();
        let query =
            visual_projection_query_from_proto(request).map_err(|status| mapping_error(&status))?;
        let result = self
            .service
            .visual_projection(query)
            .await
            .map_err(kernel_error("project_visual", &about))?;
        Ok(app_data_success_result(visual_projection_from_response(
            visual_projection_response_from_result(result),
        )))
    }
}
