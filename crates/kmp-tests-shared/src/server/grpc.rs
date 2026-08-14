use std::sync::Arc;
use std::time::Duration;

use kmp_config::{AppConfig, GrpcTlsConfig};
use kmp_domain::{
    GraphNeighborhoodReader, MemoryAboutIndexReader, NodeDetailReader, NodeRelationshipReader,
    ProjectionWriter, SnapshotStore,
};
use kmp_observability::quality_observers::NoopQualityObserver;
use kmp_testkit::InMemoryContextEventStore;
use kmp_transport_grpc::GrpcServer;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::sleep;
use tonic::transport::{Channel, Endpoint};

use crate::error::BoxError;

pub struct RunningGrpcServer {
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: tokio::task::JoinHandle<Result<(), BoxError>>,
    endpoint: String,
}

impl RunningGrpcServer {
    pub async fn start<G, D, S>(
        graph_reader: G,
        detail_reader: D,
        snapshot_store: S,
    ) -> Result<Self, BoxError>
    where
        G: GraphNeighborhoodReader
            + MemoryAboutIndexReader
            + NodeRelationshipReader
            + ProjectionWriter
            + Send
            + Sync
            + 'static,
        D: NodeDetailReader + ProjectionWriter + Send + Sync + 'static,
        S: SnapshotStore + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let endpoint = format!("http://{addr}");
        let config = AppConfig {
            service_name: "kmp".to_string(),
            grpc_bind: addr.to_string(),
            grpc_tls: GrpcTlsConfig::disabled(),
            graph_uri: "neo4j://localhost:7687".to_string(),
            detail_uri: "redis://localhost:6379".to_string(),
            snapshot_uri: "redis://localhost:6379".to_string(),
            events_subject_prefix: "rehydration".to_string(),
        };
        let server = GrpcServer::new(
            config,
            graph_reader,
            detail_reader,
            snapshot_store,
            InMemoryContextEventStore::new(),
            Arc::new(NoopQualityObserver),
        );
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let server_task = tokio::spawn(async move {
            server
                .serve_with_listener_shutdown(listener, async move {
                    let _ = shutdown_rx.await;
                })
                .await
        });

        Ok(Self {
            shutdown_tx: Some(shutdown_tx),
            server_task,
            endpoint,
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub async fn connect_channel(&self) -> Result<Channel, BoxError> {
        let mut attempts = 0u8;

        loop {
            match Endpoint::from_shared(self.endpoint.clone())?
                .connect()
                .await
            {
                Ok(channel) => return Ok(channel),
                Err(error) if attempts < 20 => {
                    attempts += 1;
                    let _ = error;
                    sleep(Duration::from_millis(25)).await;
                }
                Err(error) => return Err(Box::new(error)),
            }
        }
    }
}

pub async fn stop_server(server: RunningGrpcServer) -> Result<(), BoxError> {
    let RunningGrpcServer {
        shutdown_tx,
        server_task,
        endpoint: _,
    } = server;
    if let Some(tx) = shutdown_tx {
        let _ = tx.send(());
    }

    server_task.await?
}
