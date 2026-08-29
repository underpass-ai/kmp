use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use crate::KernelMcpServer;
use crate::guide::application::dto::guide_request_document_dto::GuideRequestDocumentDto;
use crate::guide::domain::guide_error::GuideError;
use crate::guide::ports::guide_memory_gateway::GuideMemoryGateway;

pub struct McpGuideMemoryGateway;

impl GuideMemoryGateway for McpGuideMemoryGateway {
    fn converge<'a>(
        &'a self,
        requests: &'a [GuideRequestDocumentDto],
    ) -> Pin<Box<dyn Future<Output = Result<(), GuideError>> + 'a>> {
        Box::pin(async move {
            let server = KernelMcpServer::try_from_env().map_err(GuideError::invalid)?;
            for (offset, request) in requests.iter().enumerate() {
                let identifier = u64::try_from(offset + 1).unwrap_or(u64::MAX);
                let response = server
                    .handle_json_line(&request.mcp_call(identifier).to_string())
                    .await
                    .ok_or_else(|| GuideError::invalid("guide ingest returned no MCP response"))?;
                let response: Value = serde_json::from_str(&response).map_err(|error| {
                    GuideError::invalid(format!("guide ingest returned invalid JSON: {error}"))
                })?;
                if response.get("error").is_some() || response["result"]["isError"] != false {
                    return Err(GuideError::invalid(format!(
                        "guide ingest failed for {}: {response}",
                        request.about()
                    )));
                }
            }
            Ok(())
        })
    }
}
