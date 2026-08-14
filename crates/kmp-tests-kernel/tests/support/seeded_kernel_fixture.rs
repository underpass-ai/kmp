use std::error::Error;

use kmp_proto::v1beta1::{
    context_command_service_client::ContextCommandServiceClient,
    context_query_service_client::ContextQueryServiceClient,
};
use kmp_tests_shared::containers::{Neo4jContainer, ValkeyContainer};
use kmp_tests_shared::seed::kernel_data::{seed_node_details, seed_projection_graph};
use kmp_tests_shared::server::{RunningGrpcServer, stop_server};
use tonic::transport::Channel;

pub(crate) struct SeededKernelFixture {
    _neo4j: Neo4jContainer,
    _valkey: ValkeyContainer,
    server: RunningGrpcServer,
    query_client: ContextQueryServiceClient<Channel>,
    command_client: ContextCommandServiceClient<Channel>,
}

impl SeededKernelFixture {
    pub(crate) async fn start() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let neo4j = Neo4jContainer::start().await?;
        let valkey = ValkeyContainer::start().await?;

        neo4j.clear().await?;

        let graph_reader = neo4j.graph_reader()?;
        let detail_store = valkey.detail_store()?;
        let snapshot_store = valkey.snapshot_store()?;

        seed_projection_graph(&graph_reader).await?;
        seed_node_details(&detail_store).await?;

        let server = RunningGrpcServer::start(
            graph_reader.clone(),
            detail_store.clone(),
            snapshot_store.clone(),
        )
        .await?;
        let channel = server.connect_channel().await?;

        Ok(Self {
            _neo4j: neo4j,
            _valkey: valkey,
            server,
            query_client: ContextQueryServiceClient::new(channel.clone()),
            command_client: ContextCommandServiceClient::new(channel),
        })
    }

    pub(crate) fn query_client(&mut self) -> &mut ContextQueryServiceClient<Channel> {
        &mut self.query_client
    }

    pub(crate) fn command_client(&mut self) -> &mut ContextCommandServiceClient<Channel> {
        &mut self.command_client
    }

    pub(crate) fn grpc_endpoint(&self) -> &str {
        self.server.endpoint()
    }

    pub(crate) async fn shutdown(self) -> Result<(), Box<dyn Error + Send + Sync>> {
        stop_server(self.server).await
    }
}
