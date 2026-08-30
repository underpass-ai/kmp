use crate::serving::grpc_tls_config::KernelMcpGrpcTlsConfig;

/// Which kernel a host asked for. Part of this crate's public API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KernelMcpBackend {
    Fixture,
    Grpc {
        endpoint: String,
        tls: KernelMcpGrpcTlsConfig,
    },
}
