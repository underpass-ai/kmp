use std::fmt;
use std::sync::Arc;

use kmp_ports::PortError;
use neo4rs::{ConfigBuilder, Graph};
use tokio::sync::OnceCell;

use super::endpoint::Neo4jEndpoint;

#[derive(Clone)]
pub struct Neo4jProjectionStore {
    endpoint: Neo4jEndpoint,
    graph: Arc<OnceCell<Arc<Graph>>>,
}

pub type Neo4jProjectionReader = Neo4jProjectionStore;

impl fmt::Debug for Neo4jProjectionStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Neo4jProjectionStore")
            .field("endpoint", &self.endpoint)
            .field("connected", &self.graph.get().is_some())
            .finish()
    }
}

impl Neo4jProjectionStore {
    pub fn new(graph_uri: impl Into<String>) -> Result<Self, PortError> {
        let endpoint = Neo4jEndpoint::parse(graph_uri.into())?;
        Ok(Self {
            endpoint,
            graph: Arc::new(OnceCell::new()),
        })
    }

    pub(crate) async fn graph(&self) -> Result<Arc<Graph>, PortError> {
        let graph = self
            .graph
            .get_or_try_init(|| async {
                let mut config = ConfigBuilder::new()
                    .uri(self.endpoint.connection_uri.clone())
                    .user(self.endpoint.user.clone())
                    .password(self.endpoint.password.clone());
                if let Some(tls_ca_path) = &self.endpoint.tls_ca_path {
                    config = config.with_client_certificate(tls_ca_path);
                }

                let config = config.build().map_err(|error| {
                    PortError::InvalidState(format!(
                        "neo4j configuration failed for `{}`: {error}",
                        self.endpoint.connection_uri
                    ))
                })?;

                Graph::connect(config).await.map(Arc::new).map_err(|error| {
                    PortError::Unavailable(format!(
                        "neo4j connection failed for `{}`: {error}",
                        self.endpoint.connection_uri
                    ))
                })
            })
            .await?;

        Ok(Arc::clone(graph))
    }
}
