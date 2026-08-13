use std::time::Duration;

use async_nats::Client;
use kmp_testkit::ensure_testcontainers_runtime;
use testcontainers::{
    GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use tokio::time::{sleep, timeout};

use crate::containers::ContainerEndpoint;
use crate::debug::{debug_log, debug_log_value};
use crate::error::BoxError;

const NATS_IMAGE: &str = "docker.io/library/nats";
const NATS_TAG: &str = "2.10-alpine";
const NATS_INTERNAL_PORT: u16 = 4222;
const NATS_STARTUP_WAIT: Duration = Duration::from_secs(2);
const CONNECT_RETRY_ATTEMPTS: usize = 15;
const CONNECT_RETRY_DELAY: Duration = Duration::from_secs(1);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Typed NATS container with JetStream enabled.
pub struct NatsContainer {
    container: testcontainers::ContainerAsync<GenericImage>,
    endpoint: ContainerEndpoint,
}

impl NatsContainer {
    pub async fn start() -> Result<Self, BoxError> {
        debug_log("starting nats container");
        ensure_testcontainers_runtime()?;

        let container = GenericImage::new(NATS_IMAGE, NATS_TAG)
            .with_exposed_port(NATS_INTERNAL_PORT.tcp())
            .with_wait_for(WaitFor::seconds(NATS_STARTUP_WAIT.as_secs()))
            .with_cmd(vec!["-js"])
            .start()
            .await?;

        let host = container.get_host().await?.to_string();
        let port = container.get_host_port_ipv4(NATS_INTERNAL_PORT).await?;
        let endpoint = ContainerEndpoint::new(host, port);

        Ok(Self {
            container,
            endpoint,
        })
    }

    pub fn endpoint(&self) -> &ContainerEndpoint {
        &self.endpoint
    }

    pub fn url(&self) -> String {
        self.endpoint.nats_uri()
    }

    /// Connect with retry, returning an async-nats `Client`.
    pub async fn connect(&self) -> Result<Client, BoxError> {
        connect_nats_with_retry(&self.url()).await
    }

    pub fn into_inner(self) -> testcontainers::ContainerAsync<GenericImage> {
        self.container
    }
}

/// Standalone connect function for cases where the URL is known
/// but no `NatsContainer` handle is available (e.g., TLS fixtures).
pub async fn connect_nats_with_retry(url: &str) -> Result<Client, BoxError> {
    timeout(CONNECT_TIMEOUT, connect_nats_inner(url.to_string()))
        .await
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("nats connection did not become ready within {CONNECT_TIMEOUT:?}"),
            )
        })?
}

async fn connect_nats_inner(url: String) -> Result<Client, BoxError> {
    let mut last_error: Option<BoxError> = None;

    for _ in 0..CONNECT_RETRY_ATTEMPTS {
        match async_nats::connect(&url).await {
            Ok(client) => {
                debug_log_value("connected to nats", &url);
                return Ok(client);
            }
            Err(error) => {
                debug_log_value("nats connect retry error", &error);
                last_error = Some(Box::new(error));
                sleep(CONNECT_RETRY_DELAY).await;
            }
        }
    }

    Err(last_error.expect("at least one connection attempt should fail"))
}
