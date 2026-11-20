use std::time::Duration;

use kmp_adapter_neo4j::{Neo4jProjectionReader, Neo4jProjectionStore};
use kmp_testkit::ensure_container_runtime;
use neo4rs::{Graph, query};
use tokio::time::{sleep, timeout};

use crate::containers::ContainerEndpoint;
use crate::error::BoxError;

const NEO4J_INTERNAL_PORT: u16 = 7687;
const NEO4J_IMAGE: &str = "docker.io/library/neo4j";
const NEO4J_TAG: &str = "5.26.0-community";
pub const NEO4J_PASSWORD: &str = "underpass-test-password";
const NEO4J_STARTUP_WAIT: Duration = Duration::from_secs(5);
const NEO4J_CONNECT_RETRY_ATTEMPTS: usize = 15;
const CONNECT_RETRY_DELAY: Duration = Duration::from_secs(1);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Typed Neo4j container — owns lifecycle, exposes domain-typed stores.
pub struct Neo4jContainer {
    container: kmp_testkit::RunningContainer,
    endpoint: ContainerEndpoint,
}

impl Neo4jContainer {
    pub async fn start() -> Result<Self, BoxError> {
        ensure_container_runtime()?;

        let container = kmp_testkit::ContainerSpec::new(NEO4J_IMAGE, NEO4J_TAG)
            .with_exposed_port(NEO4J_INTERNAL_PORT)
            .with_wait_for(kmp_testkit::WaitFor::seconds(NEO4J_STARTUP_WAIT.as_secs()))
            .with_env_var("NEO4J_AUTH", format!("neo4j/{NEO4J_PASSWORD}"))
            .start()
            .await?;

        let host = container.host().to_string();
        let port = container.host_port_ipv4(NEO4J_INTERNAL_PORT)?;
        let endpoint = ContainerEndpoint::new(host, port);

        Ok(Self {
            container,
            endpoint,
        })
    }

    pub fn endpoint(&self) -> &ContainerEndpoint {
        &self.endpoint
    }

    /// Read-write store (projection writer + neighborhood reader).
    pub fn graph_store(&self) -> Result<Neo4jProjectionStore, BoxError> {
        Ok(Neo4jProjectionStore::new(
            self.endpoint.neo4j_uri(NEO4J_PASSWORD),
        )?)
    }

    /// Read-only reader (neighborhood queries only).
    pub fn graph_reader(&self) -> Result<Neo4jProjectionReader, BoxError> {
        Ok(Neo4jProjectionReader::new(
            self.endpoint.neo4j_uri(NEO4J_PASSWORD),
        )?)
    }

    /// Deletes all nodes and relationships.
    pub async fn clear(&self) -> Result<(), BoxError> {
        let graph = self.connect_with_retry().await?;
        graph.run(query("MATCH (n) DETACH DELETE n")).await?;
        Ok(())
    }

    /// Raw Neo4j graph handle for direct queries.
    pub async fn connect_with_retry(&self) -> Result<Graph, BoxError> {
        connect_neo4j_with_retry(self.endpoint.neo4j_admin_uri(), "neo4j", NEO4J_PASSWORD).await
    }

    /// Keeps the underlying container alive (moved into the fixture).
    pub fn into_inner(self) -> kmp_testkit::RunningContainer {
        self.container
    }
}

async fn connect_neo4j_with_retry(
    uri: String,
    user: &str,
    password: &str,
) -> Result<Graph, BoxError> {
    timeout(
        CONNECT_TIMEOUT,
        connect_neo4j_inner(uri, user.to_string(), password.to_string()),
    )
    .await
    .map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("neo4j connection did not become ready within {CONNECT_TIMEOUT:?}"),
        )
    })?
}

async fn connect_neo4j_inner(
    uri: String,
    user: String,
    password: String,
) -> Result<Graph, BoxError> {
    let mut last_error: Option<BoxError> = None;

    for _ in 0..NEO4J_CONNECT_RETRY_ATTEMPTS {
        match ready_graph(Graph::new(&uri, &user, &password)).await {
            Ok(graph) => return Ok(graph),
            Err(error) => {
                last_error = Some(error);
                sleep(CONNECT_RETRY_DELAY).await;
            }
        }
    }

    Err(last_error.expect("at least one connection attempt should fail"))
}

/// The driver opens connections lazily and, since neo4rs 0.9, does not retry
/// a failed connection, so readiness is proven with a real round trip.
async fn ready_graph(graph: Result<Graph, neo4rs::Error>) -> Result<Graph, BoxError> {
    let graph = graph?;
    graph.run(query("RETURN 1")).await?;
    Ok(graph)
}
