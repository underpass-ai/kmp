use std::sync::Arc;
use std::time::Duration;

use kmp_config::{AppConfig, GrpcTlsConfig};
use kmp_domain::{
    ContextPathNeighborhood, GraphNeighborhoodReader, KmpBundle, MemoryAboutIndexReader,
    NodeDetailProjection, NodeDetailReader, NodeNeighborhood, NodeRelationshipReader,
    NodeRelationships, PortError, ProjectionMutation, ProjectionWriter, SnapshotSaveOptions,
    SnapshotStore,
};
use kmp_observability::quality_observers::NoopQualityObserver;
use kmp_proto::v1beta1::{
    ContextChange, ContextChangeOperation, GetContextPathRequest, GetContextRequest,
    UpdateContextRequest, context_command_service_client::ContextCommandServiceClient,
    context_query_service_client::ContextQueryServiceClient,
};
use kmp_testkit::InMemoryContextEventStore;
use kmp_transport_grpc::GrpcServer;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::sleep;
use tonic::transport::{Channel, Endpoint};

struct EmptyGraphNeighborhoodReader;

impl GraphNeighborhoodReader for EmptyGraphNeighborhoodReader {
    async fn load_neighborhood(
        &self,
        _root_node_id: &str,
        _depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        Ok(None)
    }

    async fn load_context_path(
        &self,
        _root_node_id: &str,
        _target_node_id: &str,
        _subtree_depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        Ok(None)
    }
}

impl MemoryAboutIndexReader for EmptyGraphNeighborhoodReader {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        Ok(Vec::new())
    }
}

impl NodeRelationshipReader for EmptyGraphNeighborhoodReader {
    async fn load_node_relationships(
        &self,
        _node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        Ok(None)
    }
}

impl ProjectionWriter for EmptyGraphNeighborhoodReader {
    async fn apply_mutations(&self, _mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        Ok(())
    }
}

struct EmptyNodeDetailReader;

impl NodeDetailReader for EmptyNodeDetailReader {
    async fn load_node_detail(
        &self,
        _node_id: &str,
    ) -> Result<Option<NodeDetailProjection>, PortError> {
        Ok(None)
    }

    async fn load_node_details_batch(
        &self,
        node_ids: Vec<String>,
    ) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        let mut results = Vec::with_capacity(node_ids.len());
        for node_id in &node_ids {
            results.push(self.load_node_detail(node_id).await?);
        }
        Ok(results)
    }
}

impl ProjectionWriter for EmptyNodeDetailReader {
    async fn apply_mutations(&self, _mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        Ok(())
    }
}

struct NoopSnapshotStore;

impl SnapshotStore for NoopSnapshotStore {
    async fn save_bundle_with_options(
        &self,
        _bundle: &KmpBundle,
        _options: SnapshotSaveOptions,
    ) -> Result<(), PortError> {
        Ok(())
    }
}

#[tokio::test]
async fn grpc_server_supports_query_and_command_roundtrip() {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("listener should bind: {error}"),
    };
    let addr = listener.local_addr().expect("listener should expose addr");
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
        EmptyGraphNeighborhoodReader,
        EmptyNodeDetailReader,
        NoopSnapshotStore,
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

    let endpoint = format!("http://{}", addr);
    let channel = connect_channel(endpoint).await;
    let mut query_client = ContextQueryServiceClient::new(channel.clone());
    let mut command_client = ContextCommandServiceClient::new(channel);

    let get_context_err = query_client
        .get_context(GetContextRequest {
            root_node_id: "case-123".to_string(),
            role: "developer".to_string(),
            token_budget: 1024,
            requested_scopes: vec!["decisions".to_string()],
            depth: 0,
            max_tier: 0,
            rehydration_mode: 0,
        })
        .await
        .expect_err("empty graph should return NOT_FOUND");
    assert_eq!(get_context_err.code(), tonic::Code::NotFound);

    let get_context_path_err = query_client
        .get_context_path(GetContextPathRequest {
            root_node_id: "case-123".to_string(),
            target_node_id: "case-456".to_string(),
            role: "developer".to_string(),
            token_budget: 1024,
        })
        .await
        .expect_err("empty graph should return NOT_FOUND for unknown path target");
    assert_eq!(get_context_path_err.code(), tonic::Code::NotFound);

    let _update_context = command_client
        .update_context(UpdateContextRequest {
            root_node_id: "case-123".to_string(),
            role: "developer".to_string(),
            work_item_id: "task-7".to_string(),
            changes: vec![ContextChange {
                operation: ContextChangeOperation::Update as i32,
                entity_kind: "decision".to_string(),
                entity_id: "decision-9".to_string(),
                payload_json: "{\"status\":\"accepted\"}".to_string(),
                reason: "refined".to_string(),
                scopes: vec!["decisions".to_string()],
            }],
            metadata: None,
            precondition: None,
        })
        .await
        .expect("command service should respond");

    let _ = shutdown_tx.send(());
    let result = server_task.await.expect("server task should join cleanly");
    result.expect("server should shut down cleanly");
}

async fn connect_channel(endpoint: String) -> Channel {
    let mut attempts = 0u8;

    loop {
        let connection = Endpoint::from_shared(endpoint.clone())
            .expect("endpoint should be valid")
            .connect()
            .await;

        match connection {
            Ok(channel) => return channel,
            Err(error) if attempts < 20 => {
                attempts += 1;
                sleep(Duration::from_millis(25)).await;
                let _ = error;
            }
            Err(error) => panic!("gRPC server did not become ready: {error}"),
        }
    }
}
