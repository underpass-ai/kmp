use std::error::Error;
use std::time::Duration;

use async_nats::Client;
use kmp_testkit::ensure_testcontainers_runtime;
use testcontainers::{
    GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use tokio::time::{sleep, timeout};

pub(crate) const NATS_IMAGE: &str = "docker.io/nats";
pub(crate) const NATS_TAG: &str = "2.10-alpine";
pub(crate) const NATS_INTERNAL_PORT: u16 = 4222;
const NATS_STARTUP_WAIT: Duration = Duration::from_secs(2);
const CONNECT_RETRY_ATTEMPTS: usize = 15;
const CONNECT_RETRY_DELAY: Duration = Duration::from_secs(1);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) async fn start_nats_container()
-> Result<testcontainers::ContainerAsync<GenericImage>, Box<dyn Error + Send + Sync>> {
    install_rustls_crypto_provider();
    ensure_testcontainers_runtime()?;

    Ok(GenericImage::new(NATS_IMAGE, NATS_TAG)
        .with_exposed_port(NATS_INTERNAL_PORT.tcp())
        .with_wait_for(WaitFor::seconds(NATS_STARTUP_WAIT.as_secs()))
        .with_cmd(vec!["-js"])
        .start()
        .await?)
}

pub(crate) async fn connect_with_retry(url: &str) -> Result<Client, Box<dyn Error + Send + Sync>> {
    timeout(CONNECT_TIMEOUT, connect_with_retry_inner(url.to_string()))
        .await
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "nats connection did not become ready within {:?}",
                    CONNECT_TIMEOUT
                ),
            )
        })?
}

async fn connect_with_retry_inner(url: String) -> Result<Client, Box<dyn Error + Send + Sync>> {
    let mut last_error: Option<Box<dyn Error + Send + Sync>> = None;

    for _ in 0..CONNECT_RETRY_ATTEMPTS {
        match async_nats::connect(&url).await {
            Ok(client) => return Ok(client),
            Err(error) => {
                last_error = Some(Box::new(error));
                sleep(CONNECT_RETRY_DELAY).await;
            }
        }
    }

    Err(last_error.expect("at least one connection attempt should fail"))
}

fn install_rustls_crypto_provider() {
    let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
}
