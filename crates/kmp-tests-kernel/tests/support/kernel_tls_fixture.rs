use std::error::Error;
use std::future::Future;
use std::time::Duration;

use async_nats::Client;
use kmp_adapter_nats::NatsProjectionRuntime;
use kmp_adapter_neo4j::Neo4jProjectionStore;
use kmp_adapter_valkey::{ValkeyNodeDetailStore, ValkeySnapshotStore};
use kmp_application::{ProjectionApplicationService, RoutingProjectionWriter};
use kmp_config::{GrpcTlsConfig, GrpcTlsMode};
use kmp_domain::ProjectionWriter;
use kmp_proto::v1beta1::{
    GetContextRequest, context_command_service_client::ContextCommandServiceClient,
    context_query_service_client::ContextQueryServiceClient,
};
use kmp_testkit::{InMemoryProcessedEventStore, InMemoryProjectionCheckpointStore};
use kmp_tests_shared::containers::NEO4J_PASSWORD;
use kmp_tests_shared::debug::{debug_log, debug_log_value};
use kmp_tests_shared::seed::kernel_data::DEVELOPER_ROLE;
use kmp_tests_shared::tls::grpc::{RunningTlsGrpcServer, stop_server};
use kmp_tests_shared::tls::material::TlsMaterial;
use kmp_tests_shared::tls::nats::{
    NATS_INTERNAL_PORT, client_tls_config as nats_client_tls_config, connect_with_tls_retry,
    start_nats_tls_container,
};
use kmp_tests_shared::tls::neo4j::{clear_neo4j_tls, start_neo4j_tls_container};
use kmp_tests_shared::tls::valkey::{VALKEY_INTERNAL_PORT, start_valkey_tls_container};
use testcontainers::GenericImage;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tonic::transport::Channel;

pub(crate) struct KernelTlsFixture {
    _tls: TlsMaterial,
    _neo4j: testcontainers::ContainerAsync<GenericImage>,
    _valkey: testcontainers::ContainerAsync<GenericImage>,
    _nats: testcontainers::ContainerAsync<GenericImage>,
    projection_runtime: RunningTlsProjectionRuntime,
    server: RunningTlsGrpcServer,
    query_client: ContextQueryServiceClient<Channel>,
    command_client: ContextCommandServiceClient<Channel>,
}

const NEO4J_INTERNAL_PORT_VALUE: u16 = 7687;

impl KernelTlsFixture {
    pub(crate) async fn start_with_seed<F, Fut>(
        root_node_id: &str,
        focus_node_id: &str,
        seed_projection: F,
    ) -> Result<Self, Box<dyn Error + Send + Sync>>
    where
        F: FnOnce(Client) -> Fut,
        Fut: Future<Output = Result<(), Box<dyn Error + Send + Sync>>>,
    {
        debug_log("starting kernel tls fixture");
        let tls = TlsMaterial::new()?;
        let neo4j = start_neo4j_tls_container(&tls).await?;
        let valkey = start_valkey_tls_container(&tls).await?;
        let nats = start_nats_tls_container(&tls).await?;

        let neo4j_port = neo4j.get_host_port_ipv4(NEO4J_INTERNAL_PORT_VALUE).await?;
        let valkey_port = valkey.get_host_port_ipv4(VALKEY_INTERNAL_PORT).await?;
        let nats_port = nats.get_host_port_ipv4(NATS_INTERNAL_PORT).await?;
        debug_log_value("neo4j tls host", format!("localhost:{neo4j_port}"));
        debug_log_value("valkey tls host", format!("localhost:{valkey_port}"));
        debug_log_value("nats tls host", format!("localhost:{nats_port}"));

        clear_neo4j_tls(format!("bolt+s://localhost:{neo4j_port}"), &tls.ca_cert).await?;
        debug_log("neo4j tls store cleared");

        let graph_store = Neo4jProjectionStore::new(format!(
            "bolt+s://neo4j:{NEO4J_PASSWORD}@localhost:{neo4j_port}?tls_ca_path={}",
            tls.ca_cert.display()
        ))?;
        let detail_store = ValkeyNodeDetailStore::new(format!(
            "rediss://localhost:{valkey_port}?key_prefix=rehydration:detail&ttl_seconds=120&tls_ca_path={}&tls_cert_path={}&tls_key_path={}",
            tls.ca_cert.display(),
            tls.client_cert.display(),
            tls.client_key.display()
        ))?;
        let snapshot_store = ValkeySnapshotStore::new(format!(
            "rediss://localhost:{valkey_port}?key_prefix=rehydration:snapshot&ttl_seconds=120&tls_ca_path={}&tls_cert_path={}&tls_key_path={}",
            tls.ca_cert.display(),
            tls.client_cert.display(),
            tls.client_key.display()
        ))?;

        let nats_url = format!("tls://localhost:{nats_port}");
        let projection_runtime = RunningTlsProjectionRuntime::start(
            &nats_url,
            &tls,
            "rehydration",
            graph_store.clone(),
            detail_store.clone(),
        )
        .await?;
        debug_log("tls projection runtime started");

        let server = RunningTlsGrpcServer::start(
            graph_store.clone(),
            detail_store.clone(),
            snapshot_store.clone(),
            GrpcTlsConfig {
                mode: GrpcTlsMode::Mutual,
                cert_path: Some(tls.server_cert.clone()),
                key_path: Some(tls.server_key.clone()),
                client_ca_path: Some(tls.ca_cert.clone()),
            },
        )
        .await?;
        debug_log("tls grpc server started");
        let channel = server.connect_channel(&tls, true).await?;
        debug_log("tls grpc channel connected");
        let query_client = ContextQueryServiceClient::new(channel.clone());
        let command_client = ContextCommandServiceClient::new(channel);

        let publisher = connect_with_tls_retry(&nats_url, &tls).await?;
        seed_projection(publisher.clone()).await?;
        debug_log("tls projection seed events published");
        wait_for_context_ready(query_client.clone(), root_node_id, focus_node_id).await?;
        debug_log("tls context became ready");

        Ok(Self {
            _tls: tls,
            _neo4j: neo4j,
            _valkey: valkey,
            _nats: nats,
            projection_runtime,
            server,
            query_client,
            command_client,
        })
    }

    pub(crate) fn query_client(&self) -> ContextQueryServiceClient<Channel> {
        self.query_client.clone()
    }

    pub(crate) fn command_client(&self) -> ContextCommandServiceClient<Channel> {
        self.command_client.clone()
    }

    pub(crate) async fn shutdown(self) -> Result<(), Box<dyn Error + Send + Sync>> {
        debug_log("shutting down kernel tls fixture");
        let projection_result = self.projection_runtime.shutdown().await;
        let server_result = stop_server(self.server).await;

        projection_result?;
        server_result
    }
}

struct RunningTlsProjectionRuntime {
    task: JoinHandle<Result<(), Box<dyn Error + Send + Sync>>>,
}

impl RunningTlsProjectionRuntime {
    async fn start<G, D>(
        nats_url: &str,
        tls: &TlsMaterial,
        subject_prefix: &str,
        graph_writer: G,
        detail_writer: D,
    ) -> Result<Self, Box<dyn Error + Send + Sync>>
    where
        G: ProjectionWriter + Send + Sync + 'static,
        D: ProjectionWriter + Send + Sync + 'static,
    {
        debug_log_value("tls projection runtime nats_url", nats_url);
        let runtime = NatsProjectionRuntime::connect(
            nats_url,
            &nats_client_tls_config(tls),
            subject_prefix,
            ProjectionApplicationService::new(
                RoutingProjectionWriter::new(graph_writer, detail_writer),
                InMemoryProcessedEventStore::default(),
                InMemoryProjectionCheckpointStore::default(),
            ),
        )
        .await?;
        let task = tokio::spawn(async move {
            runtime
                .run()
                .await
                .map_err(|error| Box::new(error) as Box<dyn Error + Send + Sync>)
        });

        Ok(Self { task })
    }

    async fn shutdown(self) -> Result<(), Box<dyn Error + Send + Sync>> {
        debug_log("tls projection runtime shutdown requested");
        self.task.abort();
        match self.task.await {
            Ok(result) => result,
            Err(join_error) if join_error.is_cancelled() => Ok(()),
            Err(join_error) => Err(Box::new(join_error)),
        }
    }
}

async fn wait_for_context_ready(
    mut query_client: ContextQueryServiceClient<Channel>,
    root_node_id: &str,
    _focus_node_id: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut last_error: Option<Box<dyn Error + Send + Sync>> = None;

    for _ in 0..40 {
        match query_client
            .get_context(GetContextRequest {
                root_node_id: root_node_id.to_string(),
                role: DEVELOPER_ROLE.to_string(),
                token_budget: 1200,
                requested_scopes: vec!["implementation".to_string()],
                depth: 0,
                max_tier: 0,
                rehydration_mode: 0,
            })
            .await
        {
            Ok(response) => {
                let response = response.into_inner();
                if let Some(bundle) = response.bundle
                    && bundle.root_node_id == root_node_id
                    && bundle
                        .bundles
                        .first()
                        .is_some_and(|role_bundle| !role_bundle.node_details.is_empty())
                {
                    debug_log("tls context readiness probe succeeded");
                    return Ok(());
                }
            }
            Err(error) => {
                debug_log_value("tls context readiness probe error", &error);
                last_error = Some(Box::new(error));
            }
        }

        sleep(Duration::from_millis(200)).await;
    }

    Err(last_error.unwrap_or_else(|| {
        "tls context projection did not become ready before timeout"
            .to_string()
            .into()
    }))
}
