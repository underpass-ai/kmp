use std::fs;
use std::sync::Once;

use kmp_proto::v1beta1::kernel_memory_service_client::KernelMemoryServiceClient;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};

use crate::backend::{
    GRPC_ENDPOINT_ENV, GRPC_TLS_CA_PATH_ENV, GRPC_TLS_CERT_PATH_ENV, GRPC_TLS_KEY_PATH_ENV,
    GRPC_TLS_MODE_ENV, KernelMcpGrpcTlsConfig, KernelMcpGrpcTlsMode, endpoint_uri_for_tls_mode,
};

pub(super) async fn connect_memory_client(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
) -> Result<KernelMemoryServiceClient<tonic::transport::Channel>, String> {
    connect_channel(endpoint, tls)
        .await
        .map(KernelMemoryServiceClient::new)
}

async fn connect_channel(endpoint: &str, tls: &KernelMcpGrpcTlsConfig) -> Result<Channel, String> {
    let endpoint_uri = endpoint_uri_for_tls_mode(endpoint, tls.mode);
    let mut endpoint = Endpoint::from_shared(endpoint_uri.clone()).map_err(|error| {
        format!("invalid kernel gRPC endpoint `{endpoint_uri}` from {GRPC_ENDPOINT_ENV}: {error}")
    })?;

    if tls.mode != KernelMcpGrpcTlsMode::Disabled {
        endpoint = endpoint.tls_config(client_tls_config(tls)?).map_err(|error| {
            format!(
                "invalid kernel gRPC TLS config from {GRPC_TLS_MODE_ENV}/{GRPC_TLS_CA_PATH_ENV}/{GRPC_TLS_CERT_PATH_ENV}/{GRPC_TLS_KEY_PATH_ENV}: {error}"
            )
        })?;
    }

    endpoint
        .connect()
        .await
        .map_err(|error| {
            format!(
                "failed to connect to kernel gRPC endpoint `{endpoint_uri}` from {GRPC_ENDPOINT_ENV} with TLS mode `{}`: {error}; debug={error:?}",
                tls.mode_name()
            )
        })
}

fn client_tls_config(tls: &KernelMcpGrpcTlsConfig) -> Result<ClientTlsConfig, String> {
    install_rustls_crypto_provider();

    let mut config = ClientTlsConfig::new().with_enabled_roots();

    if let Some(ca_path) = tls.ca_path.as_ref() {
        let ca_pem = fs::read(ca_path).map_err(|error| {
            format!(
                "failed to read {GRPC_TLS_CA_PATH_ENV} `{}`: {error}",
                ca_path.display()
            )
        })?;
        config = config.ca_certificate(Certificate::from_pem(ca_pem));
    }

    if let Some(domain_name) = tls.domain_name.as_deref() {
        config = config.domain_name(domain_name.to_string());
    }

    if tls.mode == KernelMcpGrpcTlsMode::Mutual {
        let cert_path = tls.cert_path.as_ref().ok_or_else(|| {
            format!("{GRPC_TLS_CERT_PATH_ENV} is required when {GRPC_TLS_MODE_ENV}=mutual")
        })?;
        let key_path = tls.key_path.as_ref().ok_or_else(|| {
            format!("{GRPC_TLS_KEY_PATH_ENV} is required when {GRPC_TLS_MODE_ENV}=mutual")
        })?;
        let cert_pem = fs::read(cert_path).map_err(|error| {
            format!(
                "failed to read {GRPC_TLS_CERT_PATH_ENV} `{}`: {error}",
                cert_path.display()
            )
        })?;
        let key_pem = fs::read(key_path).map_err(|error| {
            format!(
                "failed to read {GRPC_TLS_KEY_PATH_ENV} `{}`: {error}",
                key_path.display()
            )
        })?;
        config = config.identity(Identity::from_pem(cert_pem, key_pem));
    }

    Ok(config)
}

fn install_rustls_crypto_provider() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}
