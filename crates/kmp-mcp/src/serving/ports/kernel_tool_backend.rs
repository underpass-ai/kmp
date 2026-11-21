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

    /// Whether `kmp_ask` on this backend bridges languages inside the kernel
    /// — a lexical-bridge table is loaded beside the store — so the agent's
    /// instructions can stop asking it to translate and retry.
    fn bridges_languages(&self) -> bool {
        false
    }

    /// The tools that send stored text outside this machine on this backend:
    /// those whose TypeSafe Jev opt-in took effect for the session's store.
    /// Empty unless the owner opted in, which is how KMP ships.
    fn open_world_tools(&self) -> Vec<&'static str> {
        Vec::new()
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a>;

    /// The same call made for `caller`, the MCP host (client name, version)
    /// of the session asking. A backend that forwards to a remote kernel
    /// names it there; the others need not know.
    fn call_tool_for<'a>(
        &'a self,
        name: &'a str,
        arguments: &'a Value,
        caller: Option<(&'a str, &'a str)>,
    ) -> KernelMcpToolFuture<'a> {
        let _ = caller;
        self.call_tool(name, arguments)
    }
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

    fn bridges_languages(&self) -> bool {
        self.as_ref().bridges_languages()
    }

    fn open_world_tools(&self) -> Vec<&'static str> {
        self.as_ref().open_world_tools()
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        self.as_ref().call_tool(name, arguments)
    }

    fn call_tool_for<'a>(
        &'a self,
        name: &'a str,
        arguments: &'a Value,
        caller: Option<(&'a str, &'a str)>,
    ) -> KernelMcpToolFuture<'a> {
        self.as_ref().call_tool_for(name, arguments, caller)
    }
}
