use std::sync::Arc;

use serde_json::Value;

use crate::serving::ports::kernel_tool_future::KernelMcpToolFuture;

/// Inbound port between the MCP transport and whichever kernel answers:
/// every backend — embedded, gRPC, fixture — implements exactly this.

pub trait KernelMcpToolBackend: Send + Sync {
    fn backend_name(&self) -> &'static str;

    fn grpc_tls_mode_name(&self) -> &'static str {
        "disabled"
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a>;
}

impl<T> KernelMcpToolBackend for Arc<T>
where
    T: KernelMcpToolBackend + ?Sized,
{
    fn backend_name(&self) -> &'static str {
        self.as_ref().backend_name()
    }

    fn grpc_tls_mode_name(&self) -> &'static str {
        self.as_ref().grpc_tls_mode_name()
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        self.as_ref().call_tool(name, arguments)
    }
}
