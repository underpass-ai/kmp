use kmp_adapter_valkey::{ValkeyNodeDetailStore, ValkeySnapshotStore};
use kmp_testkit::ensure_testcontainers_runtime;
use testcontainers::{
    GenericImage,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};

use crate::containers::ContainerEndpoint;
use crate::error::BoxError;

const VALKEY_INTERNAL_PORT: u16 = 6379;
const VALKEY_IMAGE: &str = "docker.io/valkey/valkey";
const VALKEY_TAG: &str = "8.1.5-alpine";

/// Typed Valkey container — owns lifecycle, exposes domain-typed stores.
pub struct ValkeyContainer {
    container: testcontainers::ContainerAsync<GenericImage>,
    endpoint: ContainerEndpoint,
}

impl ValkeyContainer {
    pub async fn start() -> Result<Self, BoxError> {
        ensure_testcontainers_runtime()?;

        let container = GenericImage::new(VALKEY_IMAGE, VALKEY_TAG)
            .with_exposed_port(VALKEY_INTERNAL_PORT.tcp())
            .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
            .start()
            .await?;

        let host = container.get_host().await?.to_string();
        let port = container.get_host_port_ipv4(VALKEY_INTERNAL_PORT).await?;
        let endpoint = ContainerEndpoint::new(host, port);

        Ok(Self {
            container,
            endpoint,
        })
    }

    pub fn endpoint(&self) -> &ContainerEndpoint {
        &self.endpoint
    }

    pub fn detail_store(&self) -> Result<ValkeyNodeDetailStore, BoxError> {
        Ok(ValkeyNodeDetailStore::new(
            self.endpoint.redis_uri("rehydration:detail", 120),
        )?)
    }

    pub fn snapshot_store(&self) -> Result<ValkeySnapshotStore, BoxError> {
        Ok(ValkeySnapshotStore::new(
            self.endpoint.redis_uri("rehydration:snapshot", 120),
        )?)
    }

    pub fn into_inner(self) -> testcontainers::ContainerAsync<GenericImage> {
        self.container
    }
}
