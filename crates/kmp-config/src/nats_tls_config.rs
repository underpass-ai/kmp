use std::io;
use std::path::PathBuf;

use crate::env_bool::parse_bool_value;
use crate::transport_tls::{TransportTlsMode, lookup_optional_path, parse_transport_tls_mode};

pub type NatsTlsMode = TransportTlsMode;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NatsTlsConfig {
    pub mode: NatsTlsMode,
    pub ca_path: Option<PathBuf>,
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    pub tls_first: bool,
}

impl NatsTlsConfig {
    pub fn disabled() -> Self {
        Self::default()
    }

    pub(crate) fn from_lookup<F>(lookup: &F) -> io::Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        let mode = lookup("NATS_TLS_MODE")
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_transport_tls_mode("NATS_TLS_MODE", &value))
            .transpose()?
            .unwrap_or_default();

        let ca_path = lookup_optional_path(lookup, "NATS_TLS_CA_PATH");
        let cert_path = lookup_optional_path(lookup, "NATS_TLS_CERT_PATH");
        let key_path = lookup_optional_path(lookup, "NATS_TLS_KEY_PATH");
        let tls_first = lookup("NATS_TLS_FIRST")
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_bool_value(&value))
            .unwrap_or(false);

        validate_tls_first(mode, tls_first)?;
        validate_client_cert_mode(mode, &cert_path, &key_path)?;
        validate_mutual_client_cert_pair(mode, &cert_path, &key_path)?;

        Ok(Self {
            mode,
            ca_path,
            cert_path,
            key_path,
            tls_first,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NatsEndpointConfig {
    pub url: String,
    pub tls: NatsTlsConfig,
}

impl NatsEndpointConfig {
    pub(crate) fn from_lookup<F>(lookup: &F) -> io::Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        Ok(Self {
            url: lookup("NATS_URL").unwrap_or_else(|| "nats://nats:4222".to_string()),
            tls: NatsTlsConfig::from_lookup(lookup)?,
        })
    }
}

fn validate_tls_first(mode: NatsTlsMode, tls_first: bool) -> io::Result<()> {
    if !tls_first || mode != NatsTlsMode::Disabled {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "NATS_TLS_FIRST requires NATS_TLS_MODE=server or mutual",
    ))
}

fn validate_client_cert_mode(
    mode: NatsTlsMode,
    cert_path: &Option<PathBuf>,
    key_path: &Option<PathBuf>,
) -> io::Result<()> {
    if mode == NatsTlsMode::Mutual || (cert_path.is_none() && key_path.is_none()) {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "NATS_TLS_CERT_PATH and NATS_TLS_KEY_PATH are only supported when NATS_TLS_MODE=mutual",
    ))
}

fn validate_mutual_client_cert_pair(
    mode: NatsTlsMode,
    cert_path: &Option<PathBuf>,
    key_path: &Option<PathBuf>,
) -> io::Result<()> {
    if mode != NatsTlsMode::Mutual {
        return Ok(());
    }

    if cert_path.is_some() && key_path.is_some() {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "NATS_TLS_CERT_PATH and NATS_TLS_KEY_PATH are required when NATS_TLS_MODE=mutual",
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::{NatsTlsConfig, NatsTlsMode};

    #[test]
    fn server_tls_mode_and_tls_first_are_loaded() {
        let env = [
            ("NATS_TLS_MODE", "server"),
            ("NATS_TLS_CA_PATH", "/tmp/ca.pem"),
            ("NATS_TLS_FIRST", "true"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<BTreeMap<_, _>>();

        let config = NatsTlsConfig::from_lookup(&|key| env.get(key).cloned())
            .expect("TLS config should load");

        assert_eq!(config.mode, NatsTlsMode::Server);
        assert_eq!(config.ca_path.as_deref(), Some(Path::new("/tmp/ca.pem")));
        assert!(config.tls_first);
    }

    #[test]
    fn mutual_tls_loads_client_identity() {
        let env = [
            ("NATS_TLS_MODE", "mutual"),
            ("NATS_TLS_CA_PATH", "/tmp/ca.pem"),
            ("NATS_TLS_CERT_PATH", "/tmp/client.pem"),
            ("NATS_TLS_KEY_PATH", "/tmp/client.key"),
            ("NATS_TLS_FIRST", "true"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<BTreeMap<_, _>>();

        let config = NatsTlsConfig::from_lookup(&|key| env.get(key).cloned())
            .expect("mutual TLS should load");

        assert_eq!(config.mode, NatsTlsMode::Mutual);
        assert_eq!(
            config.cert_path.as_deref(),
            Some(Path::new("/tmp/client.pem"))
        );
        assert_eq!(
            config.key_path.as_deref(),
            Some(Path::new("/tmp/client.key"))
        );
        assert!(config.tls_first);
    }

    #[test]
    fn tls_validation_rejects_invalid_combinations() {
        let mutual_only = [("NATS_TLS_MODE", "mutual")]
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<BTreeMap<_, _>>();
        let tls_first_only = [("NATS_TLS_FIRST", "true")]
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<BTreeMap<_, _>>();

        let mutual_error = NatsTlsConfig::from_lookup(&|key| mutual_only.get(key).cloned())
            .expect_err("mutual TLS should require a client cert pair");
        let tls_first_error = NatsTlsConfig::from_lookup(&|key| tls_first_only.get(key).cloned())
            .expect_err("tls_first should require TLS mode");

        assert!(mutual_error.to_string().contains("NATS_TLS_CERT_PATH"));
        assert!(tls_first_error.to_string().contains("NATS_TLS_FIRST"));
    }
}
