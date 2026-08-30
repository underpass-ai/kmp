pub mod agent_policy;
pub mod banner;
pub mod clock;
pub mod document;
pub mod guide;
pub mod lifecycle;
pub mod plugin_notice;
pub mod pulse;
pub mod snapshot;
pub mod style;
pub mod viewer;

mod contract;
mod projection;
mod serving;
mod write;

pub use serving::tool_error;
pub use serving::{
    GRPC_ENDPOINT_ENV, GRPC_TLS_CA_PATH_ENV, GRPC_TLS_CERT_PATH_ENV, GRPC_TLS_DOMAIN_NAME_ENV,
    GRPC_TLS_KEY_PATH_ENV, GRPC_TLS_MODE_ENV, KernelMcpBackend, KernelMcpGrpcTlsConfig,
    KernelMcpGrpcTlsMode, KernelMcpToolBackend, KernelMcpToolFuture, MCP_BACKEND_ENV,
};
/// The tools this build advertises, in the order `tools/list` returns them.
///
/// Exposed so a diagnostic can answer "is the surface there" from inside the
/// process, instead of spawning the binary to ask it.
pub fn tool_names() -> Vec<String> {
    contract::tools_list_result()["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub use serving::FixtureKernelMcpBackend;
pub use serving::GrpcKernelMcpBackend;
pub use serving::KernelMcpServer;
pub use serving::{EmbeddedKernelMcpBackend, RetryingEmbeddedKernelMcpBackend};
pub use serving::{ToolError, ToolErrorCode};

pub fn kmp_mcp_tools_list_result() -> serde_json::Value {
    contract::tools_list_result()
}

/// The surface a host that negotiated MCP Apps is offered: the same tools plus
/// the app-only ones, and the ChronoLoom `_meta.ui` block on `kmp_view_open`.
///
/// Exposed so the advertised contract can be pinned on both paths. Pinning
/// only `apps = false` would leave the app surface — and the argument
/// rejection that reads its schemas — free to drift unnoticed.
pub fn kmp_mcp_tools_list_result_with_apps(apps: bool) -> serde_json::Value {
    contract::tools_list_result_with_apps(apps)
}

pub fn kmp_mcp_tool_names() -> Vec<String> {
    kmp_mcp_tools_list_result()
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
        let result = kmp_mcp_tools_list_result();
        let tools = result
            .get("tools")
            .and_then(serde_json::Value::as_array)
            .expect("tools list result must expose tools");

        assert_eq!(tools.len(), kmp_mcp_tool_names().len());
        assert!(kmp_mcp_tool_names().contains(&"kmp_ingest".to_string()));
        assert!(kmp_mcp_tool_names().contains(&"kmp_write_memory".to_string()));
        assert!(kmp_mcp_tool_names().contains(&"kmp_inspect".to_string()));
    }
}
