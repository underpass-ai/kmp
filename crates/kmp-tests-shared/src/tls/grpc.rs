use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use kmp_config::{AppConfig, GrpcTlsConfig};
use kmp_domain::{
    GraphNeighborhoodReader, MemoryAboutIndexReader, NodeDetailReader, NodeRelationshipReader,
    ProjectionWriter, SnapshotStore,
};
use kmp_observability::quality_observers::NoopQualityObserver;
use kmp_transport_grpc::GrpcServer;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::sleep;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};

use crate::tls::material::{TlsMaterial, ensure_crypto_provider};

pub struct RunningTlsGrpcServer {
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: tokio::task::JoinHandle<Result<(), Box<dyn Error + Send + Sync>>>,
    endpoint: String,
}

impl RunningTlsGrpcServer {
    pub async fn start<G, D, S>(
        graph_reader: G,
        detail_reader: D,
        snapshot_store: S,
        grpc_tls: GrpcTlsConfig,
    ) -> Result<Self, Box<dyn Error + Send + Sync>>
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
        let server = GrpcServer::new(
            AppConfig {
                service_name: "kmp".to_string(),
                grpc_bind: addr.to_string(),
                grpc_tls,
                graph_uri: "neo4j://localhost:7687".to_string(),
                detail_uri: "rediss://localhost:6379".to_string(),
                snapshot_uri: "rediss://localhost:6379".to_string(),
                events_subject_prefix: "rehydration".to_string(),
            },
            graph_reader,
            detail_reader,
            snapshot_store,
            kmp_testkit::InMemoryContextEventStore::new(),
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
            endpoint: format!("https://localhost:{}", addr.port()),
        })
    }

    pub async fn connect_channel(
        &self,
        tls_material: &TlsMaterial,
        include_client_identity: bool,
    ) -> Result<Channel, Box<dyn Error + Send + Sync>> {
        ensure_crypto_provider();

        let mut tls = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(tls_material.ca_certificate_pem()))
            .domain_name("localhost");
        if include_client_identity {
            let (client_cert_pem, client_key_pem) = tls_material.client_identity_pem();
            tls = tls.identity(Identity::from_pem(client_cert_pem, client_key_pem));
        }

        let endpoint = Endpoint::from_shared(self.endpoint.clone())?.tls_config(tls)?;
        let mut attempts = 0u8;

        loop {
            match endpoint.clone().connect().await {
                Ok(channel) => return Ok(channel),
                Err(error) if attempts < 20 => {
                    attempts += 1;
                    sleep(Duration::from_millis(25)).await;
                    if error.to_string().contains("Connection refused") {
                        continue;
                    }
                    return Err(Box::new(error));
                }
                Err(error) => return Err(Box::new(error)),
            }
        }
    }
}

pub async fn stop_server(server: RunningTlsGrpcServer) -> Result<(), Box<dyn Error + Send + Sync>> {
    let RunningTlsGrpcServer {
        shutdown_tx,
        server_task,
        endpoint: _,
    } = server;
    if let Some(tx) = shutdown_tx {
        let _ = tx.send(());
    }

    server_task.await?
}
