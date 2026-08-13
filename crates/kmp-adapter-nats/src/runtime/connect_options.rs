use std::path::PathBuf;

use async_nats::{Client, ConnectOptions};

use crate::runtime::runtime_error::NatsRuntimeError;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NatsClientTlsConfig {
    pub require_tls: bool,
    pub ca_path: Option<PathBuf>,
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    pub tls_first: bool,
}

impl NatsClientTlsConfig {
    pub fn disabled() -> Self {
        Self::default()
    }
}

pub async fn connect_nats_client(
    url: &str,
    tls: &NatsClientTlsConfig,
) -> Result<Client, NatsRuntimeError> {
    let options = build_connect_options(tls)?;

    options
        .connect(url)
        .await
        .map_err(|error| NatsRuntimeError::Connection(error.to_string()))
}

fn build_connect_options(tls: &NatsClientTlsConfig) -> Result<ConnectOptions, NatsRuntimeError> {
    validate_tls_config(tls)?;

    let mut options = ConnectOptions::new();

    if tls.require_tls {
        options = options.require_tls(true);
    }

    if let Some(ca_path) = &tls.ca_path {
        options = options.add_root_certificates(ca_path.clone());
    }

    if let (Some(cert_path), Some(key_path)) = (&tls.cert_path, &tls.key_path) {
        options = options.add_client_certificate(cert_path.clone(), key_path.clone());
    }

    if tls.tls_first {
        options = options.tls_first();
    }

    Ok(options)
}

fn validate_tls_config(tls: &NatsClientTlsConfig) -> Result<(), NatsRuntimeError> {
    match (&tls.cert_path, &tls.key_path) {
        (Some(_), Some(_)) | (None, None) => {}
        _ => {
            return Err(NatsRuntimeError::Connection(
                "nats TLS client certificate and key must be configured together".to_string(),
            ));
        }
    }

    if tls.tls_first && !tls.require_tls {
        return Err(NatsRuntimeError::Connection(
            "nats TLS-first mode requires TLS to be enabled".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{NatsClientTlsConfig, build_connect_options, validate_tls_config};

    #[test]
    fn tls_first_requires_tls() {
        let error = validate_tls_config(&NatsClientTlsConfig {
            tls_first: true,
            ..NatsClientTlsConfig::disabled()
        })
        .expect_err("tls_first without TLS should fail");

        assert!(error.to_string().contains("TLS-first"));
    }

    #[test]
    fn client_certificate_requires_key_pair() {
        let error = build_connect_options(&NatsClientTlsConfig {
            require_tls: true,
            cert_path: Some("/tmp/client.crt".into()),
            ..NatsClientTlsConfig::disabled()
        })
        .expect_err("partial client identity should fail");

        assert!(error.to_string().contains("certificate and key"));
    }
}
