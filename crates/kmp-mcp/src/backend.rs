use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use serde_json::Value;

pub const GRPC_ENDPOINT_ENV: &str = "KMP_KERNEL_GRPC_ENDPOINT";
pub const MCP_BACKEND_ENV: &str = "KMP_MCP_BACKEND";
pub const GRPC_TLS_MODE_ENV: &str = "KMP_KERNEL_GRPC_TLS_MODE";
pub const GRPC_TLS_CA_PATH_ENV: &str = "KMP_KERNEL_GRPC_TLS_CA_PATH";
pub const GRPC_TLS_CERT_PATH_ENV: &str = "KMP_KERNEL_GRPC_TLS_CERT_PATH";
pub const GRPC_TLS_KEY_PATH_ENV: &str = "KMP_KERNEL_GRPC_TLS_KEY_PATH";
pub const GRPC_TLS_DOMAIN_NAME_ENV: &str = "KMP_KERNEL_GRPC_TLS_DOMAIN_NAME";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KernelMcpBackend {
    Fixture,
    Grpc {
        endpoint: String,
        tls: KernelMcpGrpcTlsConfig,
    },
}

pub type KernelMcpToolFuture<'a> = Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'a>>;

pub trait KernelMcpToolBackend: Send + Sync {
    fn backend_name(&self) -> &'static str;

    fn grpc_tls_mode_name(&self) -> &'static str {
        "disabled"
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KernelMcpGrpcTlsConfig {
    pub(crate) mode: KernelMcpGrpcTlsMode,
    pub(crate) ca_path: Option<PathBuf>,
    pub(crate) cert_path: Option<PathBuf>,
    pub(crate) key_path: Option<PathBuf>,
    pub(crate) domain_name: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KernelMcpGrpcTlsMode {
    Disabled,
    Server,
    Mutual,
}

impl KernelMcpGrpcTlsMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Server => "server",
            Self::Mutual => "mutual",
        }
    }
}

impl KernelMcpGrpcTlsConfig {
    pub fn disabled() -> Self {
        Self {
            mode: KernelMcpGrpcTlsMode::Disabled,
            ca_path: None,
            cert_path: None,
            key_path: None,
            domain_name: None,
        }
    }

    pub fn server(ca_path: impl Into<PathBuf>, domain_name: Option<String>) -> Self {
        Self {
            mode: KernelMcpGrpcTlsMode::Server,
            ca_path: Some(ca_path.into()),
            cert_path: None,
            key_path: None,
            domain_name,
        }
    }

    pub fn mutual(
        ca_path: impl Into<PathBuf>,
        cert_path: impl Into<PathBuf>,
        key_path: impl Into<PathBuf>,
        domain_name: Option<String>,
    ) -> Self {
        Self {
            mode: KernelMcpGrpcTlsMode::Mutual,
            ca_path: Some(ca_path.into()),
            cert_path: Some(cert_path.into()),
            key_path: Some(key_path.into()),
            domain_name,
        }
    }

    pub fn from_env_for_endpoint(endpoint: Option<&str>) -> Self {
        let ca_path = optional_env_path(GRPC_TLS_CA_PATH_ENV);
        let cert_path = optional_env_path(GRPC_TLS_CERT_PATH_ENV);
        let key_path = optional_env_path(GRPC_TLS_KEY_PATH_ENV);
        let domain_name = optional_env_string(GRPC_TLS_DOMAIN_NAME_ENV);
        let server_tls_requested = ca_path.is_some()
            || domain_name.is_some()
            || endpoint
                .map(|endpoint| endpoint.trim().starts_with("https://"))
                .unwrap_or(false);
        let mode = optional_env_string(GRPC_TLS_MODE_ENV)
            .and_then(|value| parse_tls_mode(&value))
            .unwrap_or_else(|| {
                if cert_path.is_some() || key_path.is_some() {
                    KernelMcpGrpcTlsMode::Mutual
                } else if server_tls_requested {
                    KernelMcpGrpcTlsMode::Server
                } else {
                    KernelMcpGrpcTlsMode::Disabled
                }
            });

        Self {
            mode,
            ca_path,
            cert_path,
            key_path,
            domain_name,
        }
    }

    pub fn from_env() -> Self {
        Self::from_env_for_endpoint(std::env::var(GRPC_ENDPOINT_ENV).ok().as_deref())
    }

    pub fn mode(&self) -> KernelMcpGrpcTlsMode {
        self.mode
    }

    pub fn mode_name(&self) -> &'static str {
        self.mode.as_str()
    }
}

pub(crate) fn endpoint_uri_for_tls_mode(endpoint: &str, mode: KernelMcpGrpcTlsMode) -> String {
    if mode == KernelMcpGrpcTlsMode::Disabled {
        return endpoint.to_string();
    }

    endpoint
        .strip_prefix("http://")
        .map(|without_scheme| format!("https://{without_scheme}"))
        .unwrap_or_else(|| endpoint.to_string())
}

fn parse_tls_mode(value: &str) -> Option<KernelMcpGrpcTlsMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" => None,
        "disabled" | "disable" | "off" | "false" | "none" => Some(KernelMcpGrpcTlsMode::Disabled),
        "server" | "tls" => Some(KernelMcpGrpcTlsMode::Server),
        "mutual" | "mtls" | "m-tls" => Some(KernelMcpGrpcTlsMode::Mutual),
        _ => None,
    }
}

fn optional_env_path(name: &str) -> Option<PathBuf> {
    optional_env_string(name).map(PathBuf::from)
}

fn optional_env_string(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_mode_names_are_stable_for_logs_and_metadata() {
        assert_eq!(KernelMcpGrpcTlsMode::Disabled.as_str(), "disabled");
        assert_eq!(KernelMcpGrpcTlsMode::Server.as_str(), "server");
        assert_eq!(KernelMcpGrpcTlsMode::Mutual.as_str(), "mutual");
    }

    #[test]
    fn tls_endpoint_uri_upgrades_http_when_tls_is_enabled() {
        assert_eq!(
            endpoint_uri_for_tls_mode("http://127.0.0.1:50051", KernelMcpGrpcTlsMode::Server),
            "https://127.0.0.1:50051"
        );
        assert_eq!(
            endpoint_uri_for_tls_mode("https://kernel.example.test", KernelMcpGrpcTlsMode::Mutual),
            "https://kernel.example.test"
        );
        assert_eq!(
            endpoint_uri_for_tls_mode("http://127.0.0.1:50051", KernelMcpGrpcTlsMode::Disabled),
            "http://127.0.0.1:50051"
        );
    }

    #[test]
    fn tls_constructors_preserve_mode_and_paths() {
        let disabled = KernelMcpGrpcTlsConfig::disabled();
        assert_eq!(disabled.mode(), KernelMcpGrpcTlsMode::Disabled);
        assert_eq!(disabled.mode_name(), "disabled");

        let server = KernelMcpGrpcTlsConfig::server("/tmp/ca.pem", Some("kernel.local".into()));
        assert_eq!(server.mode(), KernelMcpGrpcTlsMode::Server);
        assert_eq!(
            server.ca_path.as_deref(),
            Some(std::path::Path::new("/tmp/ca.pem"))
        );
        assert_eq!(server.domain_name.as_deref(), Some("kernel.local"));

        let mutual =
            KernelMcpGrpcTlsConfig::mutual("/tmp/ca.pem", "/tmp/cert.pem", "/tmp/key.pem", None);
        assert_eq!(mutual.mode(), KernelMcpGrpcTlsMode::Mutual);
        assert_eq!(
            mutual.cert_path.as_deref(),
            Some(std::path::Path::new("/tmp/cert.pem"))
        );
        assert_eq!(
            mutual.key_path.as_deref(),
            Some(std::path::Path::new("/tmp/key.pem"))
        );
    }
}
