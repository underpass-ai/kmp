pub mod banner;
pub mod clock;
pub mod diagnostics;
pub mod document;
pub mod viewer;

mod args;
mod backend;
mod embedded;
mod fixture;
mod grpc;
mod ingest;
mod kmp;
mod observability;
mod protocol;
mod recall_projection;
mod server;
mod write;

pub use backend::{
    GRPC_ENDPOINT_ENV, GRPC_TLS_CA_PATH_ENV, GRPC_TLS_CERT_PATH_ENV, GRPC_TLS_DOMAIN_NAME_ENV,
    GRPC_TLS_KEY_PATH_ENV, GRPC_TLS_MODE_ENV, KernelMcpBackend, KernelMcpGrpcTlsConfig,
    KernelMcpGrpcTlsMode, KernelMcpToolBackend, KernelMcpToolFuture, MCP_BACKEND_ENV,
};
/// The tools this build advertises, in the order `tools/list` returns them.
///
/// Exposed so a diagnostic can answer "is the surface there" from inside the
/// process, instead of spawning the binary to ask it.
pub fn tool_names() -> Vec<String> {
    protocol::tools_list_result()["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub use embedded::{EmbeddedKernelMcpBackend, RetryingEmbeddedKernelMcpBackend};
pub use fixture::FixtureKernelMcpBackend;
pub use grpc::GrpcKernelMcpBackend;
pub use server::KernelMcpServer;

pub fn kernel_mcp_tools_list_result() -> serde_json::Value {
    protocol::tools_list_result()
}

pub fn kernel_mcp_tool_names() -> Vec<String> {
    kernel_mcp_tools_list_result()
        .get("tools")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tool| tool.get("name").and_then(serde_json::Value::as_str))
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_tool_helpers_expose_the_kmp_tool_contract() {
        let result = kernel_mcp_tools_list_result();
        let tools = result
            .get("tools")
            .and_then(serde_json::Value::as_array)
            .expect("tools list result must expose tools");

        assert_eq!(tools.len(), kernel_mcp_tool_names().len());
        assert!(kernel_mcp_tool_names().contains(&"kernel_ingest".to_string()));
        assert!(kernel_mcp_tool_names().contains(&"kernel_write_memory".to_string()));
        assert!(kernel_mcp_tool_names().contains(&"kernel_inspect".to_string()));
    }
}
