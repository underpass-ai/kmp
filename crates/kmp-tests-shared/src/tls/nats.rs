use std::error::Error;
use std::fs;
use std::time::Duration;

use async_nats::{Client, ConnectOptions};
use kmp_adapter_nats::NatsClientTlsConfig;
use kmp_testkit::ensure_testcontainers_runtime;
use testcontainers::{
    GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use tokio::time::{sleep, timeout};

use crate::debug::{debug_log, debug_log_value};
use crate::tls::material::{TlsMaterial, ensure_crypto_provider};

pub const NATS_IMAGE: &str = "docker.io/library/nats";
pub const NATS_TAG: &str = "2.10-alpine";
pub const NATS_INTERNAL_PORT: u16 = 4222;
const NATS_STARTUP_WAIT: Duration = Duration::from_secs(2);
const CONNECT_RETRY_ATTEMPTS: usize = 15;
const CONNECT_RETRY_DELAY: Duration = Duration::from_secs(1);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

pub async fn start_nats_tls_container(
    tls_material: &TlsMaterial,
) -> Result<testcontainers::ContainerAsync<GenericImage>, Box<dyn Error + Send + Sync>> {
    debug_log("starting tls nats container");
    ensure_testcontainers_runtime()?;
    fs::write(
        tls_material.dir().join("nats.conf"),
        r#"listen: 0.0.0.0:4222

tls {
  cert_file: "/tls/server.crt"
  key_file: "/tls/server.key"
  ca_file: "/tls/ca.crt"
  verify: true
  timeout: 2
}
"#,
    )?;

    Ok(GenericImage::new(NATS_IMAGE, NATS_TAG)
        .with_entrypoint("nats-server")
        .with_exposed_port(NATS_INTERNAL_PORT.tcp())
        .with_wait_for(WaitFor::seconds(NATS_STARTUP_WAIT.as_secs()))
        .with_copy_to("/tls", tls_material.dir())
        .with_cmd(vec!["-js", "-c", "/tls/nats.conf"])
        .start()
        .await?)
}

pub async fn connect_with_tls_retry(
    url: &str,
    tls_material: &TlsMaterial,
) -> Result<Client, Box<dyn Error + Send + Sync>> {
    timeout(
        CONNECT_TIMEOUT,
        connect_with_tls_retry_inner(url.to_string(), tls_material),
    )
    .await
    .map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!(
                "nats TLS connection did not become ready within {:?}",
                CONNECT_TIMEOUT
            ),
        )
    })?
}

pub fn client_tls_config(tls_material: &TlsMaterial) -> NatsClientTlsConfig {
    NatsClientTlsConfig {
        require_tls: true,
        ca_path: Some(tls_material.ca_cert.clone()),
        cert_path: Some(tls_material.client_cert.clone()),
        key_path: Some(tls_material.client_key.clone()),
        tls_first: false,
    }
}

async fn connect_with_tls_retry_inner(
    url: String,
    tls_material: &TlsMaterial,
) -> Result<Client, Box<dyn Error + Send + Sync>> {
    let mut last_error: Option<Box<dyn Error + Send + Sync>> = None;

    for _ in 0..CONNECT_RETRY_ATTEMPTS {
        match connect_tls_client(&url, tls_material).await {
            Ok(client) => {
                debug_log_value("connected to tls nats", url);
                return Ok(client);
            }
            Err(error) => {
                debug_log_value("tls nats connect retry error", &error);
                last_error = Some(error);
                sleep(CONNECT_RETRY_DELAY).await;
            }
        }
    }

    Err(last_error.expect("at least one connection attempt should fail"))
}

async fn connect_tls_client(
    url: &str,
    tls_material: &TlsMaterial,
) -> Result<Client, Box<dyn Error + Send + Sync>> {
    ensure_crypto_provider();
    let options = ConnectOptions::new()
        .add_root_certificates(tls_material.ca_cert.clone())
        .add_client_certificate(
            tls_material.client_cert.clone(),
            tls_material.client_key.clone(),
        )
        .require_tls(true);

    options
        .connect(url)
        .await
        .map_err(|error| Box::new(error) as Box<dyn Error + Send + Sync>)
}
