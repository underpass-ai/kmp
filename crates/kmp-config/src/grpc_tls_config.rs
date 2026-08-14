use std::io;
use std::path::PathBuf;

use crate::transport_tls::{TransportTlsMode, lookup_optional_path, parse_transport_tls_mode};

pub type GrpcTlsMode = TransportTlsMode;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GrpcTlsConfig {
    pub mode: GrpcTlsMode,
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    pub client_ca_path: Option<PathBuf>,
}

impl GrpcTlsConfig {
    pub fn disabled() -> Self {
        Self::default()
    }

    pub(crate) fn from_lookup<F>(lookup: &F) -> io::Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        let mode = lookup("KMP_GRPC_TLS_MODE")
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_transport_tls_mode("KMP_GRPC_TLS_MODE", &value))
            .transpose()?
            .unwrap_or_default();

        let cert_path = lookup_optional_path(lookup, "KMP_GRPC_TLS_CERT_PATH");
        let key_path = lookup_optional_path(lookup, "KMP_GRPC_TLS_KEY_PATH");
        let client_ca_path = lookup_optional_path(lookup, "KMP_GRPC_TLS_CLIENT_CA_PATH");

        validate_required_path(mode, "KMP_GRPC_TLS_CERT_PATH", &cert_path)?;
        validate_required_path(mode, "KMP_GRPC_TLS_KEY_PATH", &key_path)?;
        if mode == GrpcTlsMode::Mutual {
            validate_required_client_ca_path(&client_ca_path)?;
        }

        Ok(Self {
            mode,
            cert_path,
            key_path,
            client_ca_path,
        })
    }
}

fn validate_required_path(mode: GrpcTlsMode, key: &str, path: &Option<PathBuf>) -> io::Result<()> {
    if mode == GrpcTlsMode::Disabled || path.is_some() {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{key} is required when KMP_GRPC_TLS_MODE={}", mode.as_str()),
    ))
}

fn validate_required_client_ca_path(path: &Option<PathBuf>) -> io::Result<()> {
    if path.is_some() {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "KMP_GRPC_TLS_CLIENT_CA_PATH is required when KMP_GRPC_TLS_MODE=mutual",
    ))
}
