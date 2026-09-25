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
/// Bearer key for the optional TypeSafe judgement adapter. Never logged.
pub const TYPESAFE_API_KEY_ENV: &str = "TYPESAFE_API_KEY";
/// Evaluation only: a file of recorded TypeSafe judgements. With
/// `KMP_TYPESAFE_CASSETTE_MODE=record` real answers are appended to it; in
/// the default `replay` mode they are answered from it, with no network and
/// no key, and an unrecorded request fails instead of being guessed.
pub const TYPESAFE_CASSETTE_ENV: &str = "KMP_TYPESAFE_CASSETTE";
pub const TYPESAFE_CASSETTE_MODE_ENV: &str = "KMP_TYPESAFE_CASSETTE_MODE";

pub(crate) fn optional_env_path(name: &str) -> Option<PathBuf> {
    optional_env_string(name).map(PathBuf::from)
}

pub(crate) fn optional_env_string(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
