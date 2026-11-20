use kmp_adapter_valkey::{ValkeyNodeDetailStore, ValkeySnapshotStore};
use kmp_testkit::ensure_container_runtime;

use crate::containers::ContainerEndpoint;
use crate::error::BoxError;

const VALKEY_INTERNAL_PORT: u16 = 6379;
const VALKEY_IMAGE: &str = "docker.io/valkey/valkey";
const VALKEY_TAG: &str = "8.1.5-alpine";

/// Typed Valkey container — owns lifecycle, exposes domain-typed stores.
pub struct ValkeyContainer {
    container: kmp_testkit::RunningContainer,
    endpoint: ContainerEndpoint,
}

impl ValkeyContainer {
    pub async fn start() -> Result<Self, BoxError> {
        ensure_container_runtime()?;

        let container = kmp_testkit::ContainerSpec::new(VALKEY_IMAGE, VALKEY_TAG)
            .with_exposed_port(VALKEY_INTERNAL_PORT)
            .with_wait_for(kmp_testkit::WaitFor::message_on_stdout(
                "Ready to accept connections",
            ))
            .start()
            .await?;

        let host = container.host().to_string();
        let port = container.host_port_ipv4(VALKEY_INTERNAL_PORT)?;
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

    pub fn into_inner(self) -> kmp_testkit::RunningContainer {
        self.container
    }
}
