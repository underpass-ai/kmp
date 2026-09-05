//! The serving bounded context: the MCP boundary itself. JSON-RPC framing,
//! the transport server and its dispatch, the backend port and its
//! adapters, the tool error vocabulary, and result envelopes.

pub(crate) mod adapters;
pub(crate) mod backend_choice;
pub(crate) mod environment;
mod existing_entry_read;
pub(crate) mod grpc_tls_config;
pub(crate) mod grpc_tls_mode;
pub(crate) mod json_rpc;
pub(crate) mod kernel_mcp_server;
pub(crate) mod ports;
pub(crate) mod projection_names;
mod relabel_dispatch;
mod rpc_dispatch;
pub(crate) mod telemetry;
pub mod tool_error;
pub(crate) mod tool_error_code;
pub(crate) mod tool_result;
pub(crate) mod unhonored_projection;
mod view_dispatch;
pub(crate) mod view_tools;
mod write_dispatch;

pub use adapters::embedded_backend::EmbeddedKernelMcpBackend;
pub use adapters::fixture_backend::FixtureKernelMcpBackend;
pub use adapters::grpc::GrpcKernelMcpBackend;
pub use adapters::retrying_embedded_backend::RetryingEmbeddedKernelMcpBackend;
pub use backend_choice::KernelMcpBackend;
pub use environment::{
    GRPC_ENDPOINT_ENV, GRPC_TLS_CA_PATH_ENV, GRPC_TLS_CERT_PATH_ENV, GRPC_TLS_DOMAIN_NAME_ENV,
    GRPC_TLS_KEY_PATH_ENV, GRPC_TLS_MODE_ENV, MCP_BACKEND_ENV,
};
pub use grpc_tls_config::KernelMcpGrpcTlsConfig;
pub(crate) use grpc_tls_config::endpoint_uri_for_tls_mode;
pub use grpc_tls_mode::KernelMcpGrpcTlsMode;
pub use kernel_mcp_server::KernelMcpServer;
pub use ports::kernel_tool_backend::KernelMcpToolBackend;
pub use ports::kernel_tool_future::KernelMcpToolFuture;
pub use tool_error::ToolError;
pub use tool_error_code::ToolErrorCode;
pub(crate) use tool_result::{app_data_success_result, tool_success_result};
