use super::super::embedded_errors::{kernel_error, mapping_error};
use crate::serving::adapters::tool_request_mapping::MemoryNodesRequestMapper;
use crate::serving::{ToolError, app_data_success_result};
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::{
    memory_nodes_json, memory_nodes_request_from_proto, memory_nodes_response_from_result,
};
use serde_json::Value;

pub(crate) struct EmbeddedMemoryNodesTool<'a> {
    service: &'a EmbeddedMemoryService,
}
impl<'a> EmbeddedMemoryNodesTool<'a> {
    pub(crate) fn new(service: &'a EmbeddedMemoryService) -> Self {
        Self { service }
    }
    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request = MemoryNodesRequestMapper::from_arguments(arguments)
            .map_err(ToolError::invalid_argument)?;
        let about = request.about.clone();
        let query = memory_nodes_request_from_proto(request).map_err(|e| mapping_error(&e))?;
        let result = self
            .service
            .read_nodes(query)
            .await
            .map_err(kernel_error("read_nodes", &about))?;
        Ok(app_data_success_result(memory_nodes_json(
            memory_nodes_response_from_result(result),
        )))
    }
}
