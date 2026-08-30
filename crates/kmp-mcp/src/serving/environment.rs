//! The environment variables through which a host selects and secures a
//! backend, and the two readings every consumer must agree on.

use std::path::PathBuf;

pub const GRPC_ENDPOINT_ENV: &str = "KMP_KERNEL_GRPC_ENDPOINT";
pub const MCP_BACKEND_ENV: &str = "KMP_MCP_BACKEND";
pub const GRPC_TLS_MODE_ENV: &str = "KMP_KERNEL_GRPC_TLS_MODE";
pub const GRPC_TLS_CA_PATH_ENV: &str = "KMP_KERNEL_GRPC_TLS_CA_PATH";
pub const GRPC_TLS_CERT_PATH_ENV: &str = "KMP_KERNEL_GRPC_TLS_CERT_PATH";
pub const GRPC_TLS_KEY_PATH_ENV: &str = "KMP_KERNEL_GRPC_TLS_KEY_PATH";
pub const GRPC_TLS_DOMAIN_NAME_ENV: &str = "KMP_KERNEL_GRPC_TLS_DOMAIN_NAME";

pub(crate) fn optional_env_path(name: &str) -> Option<PathBuf> {
    optional_env_string(name).map(PathBuf::from)
}

pub(crate) fn optional_env_string(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
