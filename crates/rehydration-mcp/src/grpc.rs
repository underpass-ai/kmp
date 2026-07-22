mod channel;
pub(crate) mod requests;
mod temporal;
mod tools;

use serde_json::Value;

use crate::backend::{KernelMcpGrpcTlsConfig, KernelMcpToolBackend, KernelMcpToolFuture};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GrpcKernelMcpBackend {
    endpoint: String,
    tls: KernelMcpGrpcTlsConfig,
}

impl GrpcKernelMcpBackend {
    pub fn new(endpoint: impl Into<String>, tls: KernelMcpGrpcTlsConfig) -> Self {
        Self {
            endpoint: endpoint.into(),
            tls,
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn tls(&self) -> &KernelMcpGrpcTlsConfig {
        &self.tls
    }
}

impl KernelMcpToolBackend for GrpcKernelMcpBackend {
    fn backend_name(&self) -> &'static str {
        "grpc"
    }

    fn grpc_tls_mode_name(&self) -> &'static str {
        self.tls.mode_name()
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        Box::pin(async move {
            tools::grpc_tool_result(&self.endpoint, &self.tls, name, arguments).await
        })
    }
}
