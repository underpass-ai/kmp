use std::sync::Arc;

use kmp_mcp::KernelMcpServer;
use kmp_mcp_http::auth::OidcJwtVerifier;
use kmp_mcp_http::config::HttpGatewayConfig;
use kmp_mcp_http::{AppState, router};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("kmp_mcp_http=info,kmp_mcp=info")),
        )
        .init();

    let config = HttpGatewayConfig::from_env().map_err(startup_error)?;
    let server = KernelMcpServer::try_from_env().map_err(startup_error)?;
    if server.backend_name() != "grpc" {
        return Err(startup_error(
            "the HTTP gateway only supports the remote gRPC KMP backend",
        ));
    }
    if config.require_grpc_mtls && server.grpc_tls_mode_name() != "mutual" {
        return Err(startup_error(
            "production HTTP gateway requires mutual TLS to the gRPC backend",
        ));
    }
    let verifier = OidcJwtVerifier::discover(
        config.issuer.clone(),
        config.audience.clone(),
        config.jwks_uri.clone(),
    )
    .await
    .map_err(|error| startup_error(format!("OIDC startup failed: {error:?}")))?;
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!(
        address = %listener.local_addr()?,
        public_url = %config.public_url,
        grpc_tls = server.grpc_tls_mode_name(),
        "KMP Streamable HTTP gateway ready"
    );
    axum::serve(
        listener,
        router(AppState::new(config, server, Arc::new(verifier))),
    )
    .await?;
    Ok(())
}

fn startup_error(message: impl Into<String>) -> Box<dyn std::error::Error> {
    std::io::Error::other(message.into()).into()
}
