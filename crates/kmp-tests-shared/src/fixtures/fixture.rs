use std::time::Duration;

use kmp_proto::v1beta1::{
    GetContextRequest, context_command_service_client::ContextCommandServiceClient,
    context_query_service_client::ContextQueryServiceClient,
};
use tokio::time::sleep;
use tonic::transport::Channel;

use crate::containers::{NatsContainer, Neo4jContainer, ValkeyContainer};
use crate::debug::{debug_log, debug_log_value};
use crate::error::BoxError;
use crate::ports::SeedContext;
use crate::runtime::RunningProjectionRuntime;
use crate::server::{RunningGrpcServer, stop_server};

/// Unified test fixture — replaces `AgenticFixture`, `SeededKernelFixture`,
/// and `KernelTlsFixture` with a composable, builder-driven orchestrator.
///
/// Owns container lifecycles and exposes gRPC clients for test assertions.
pub struct TestFixture {
    // Typed container wrappers — kept alive for the fixture's lifetime.
    neo4j: Option<Neo4jContainer>,
    #[allow(dead_code)]
    valkey: Option<ValkeyContainer>,
    nats: Option<NatsContainer>,

    projection_runtime: Option<RunningProjectionRuntime>,
    server: Option<RunningGrpcServer>,

    nats_url: Option<String>,
    query_client: Option<ContextQueryServiceClient<Channel>>,
    command_client: Option<ContextCommandServiceClient<Channel>>,
}

impl TestFixture {
    pub fn builder() -> super::TestFixtureBuilder {
        super::TestFixtureBuilder::new()
    }

    pub(crate) fn new(
        neo4j: Option<Neo4jContainer>,
        valkey: Option<ValkeyContainer>,
        nats: Option<NatsContainer>,
        projection_runtime: Option<RunningProjectionRuntime>,
        server: Option<RunningGrpcServer>,
        query_client: Option<ContextQueryServiceClient<Channel>>,
        command_client: Option<ContextCommandServiceClient<Channel>>,
    ) -> Self {
        let nats_url = nats.as_ref().map(|n| n.url());
        Self {
            neo4j,
            valkey,
            nats,
            projection_runtime,
            server,
            nats_url,
            query_client,
            command_client,
        }
    }

    pub fn query_client(&self) -> ContextQueryServiceClient<Channel> {
        self.query_client
            .clone()
            .expect("TestFixture: query_client not available — call .with_grpc_server()")
    }

    pub fn command_client(&self) -> ContextCommandServiceClient<Channel> {
        self.command_client
            .clone()
            .expect("TestFixture: command_client not available — call .with_grpc_server()")
    }

    pub fn nats_url(&self) -> &str {
        self.nats_url
            .as_deref()
            .expect("TestFixture: nats not available — call .with_nats()")
    }

    /// Re-seed the graph without recreating containers.
    pub async fn reseed(
        &self,
        seed: &dyn crate::ports::SeedStrategy,
        root_node_id: &str,
        focus_node_id: &str,
    ) -> Result<(), BoxError> {
        debug_log("reseed: clearing neo4j");
        if let Some(ref neo4j) = self.neo4j {
            neo4j.clear().await?;
        }

        debug_log("reseed: publishing new seed data");
        let nats_client = if let Some(ref n) = self.nats {
            Some(n.connect().await?)
        } else {
            None
        };
        let ctx = SeedContext::new(nats_client);
        seed.seed(&ctx).await?;
        debug_log("reseed: events published");

        if let Some(ref qc) = self.query_client {
            wait_for_context_ready(qc.clone(), root_node_id, focus_node_id, true).await?;
            debug_log("reseed: context ready");
        }
        Ok(())
    }

    pub async fn shutdown(self) -> Result<(), BoxError> {
        debug_log("shutting down test fixture");
        if let Some(runtime) = self.projection_runtime {
            runtime.shutdown().await?;
        }
        if let Some(server) = self.server {
            stop_server(server).await?;
        }
        Ok(())
    }
}

/// Polls GetContext until the projection has populated the graph.
pub(crate) async fn wait_for_context_ready(
    mut query_client: ContextQueryServiceClient<Channel>,
    root_node_id: &str,
    focus_node_id: &str,
    require_node_detail: bool,
) -> Result<(), BoxError> {
    let _ = focus_node_id; // retained for API compatibility
    let mut last_error: Option<BoxError> = None;

    for _ in 0..40 {
        match query_client
            .get_context(GetContextRequest {
                root_node_id: root_node_id.to_string(),
                role: "implementer".to_string(),
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
                    && bundle.bundles.first().is_some_and(|role_bundle| {
                        !role_bundle.neighbor_nodes.is_empty()
                            && (!require_node_detail || !role_bundle.node_details.is_empty())
                    })
                {
                    debug_log("context readiness probe succeeded");
                    return Ok(());
                }
            }
            Err(error) => {
                debug_log_value("context readiness probe error", &error);
                last_error = Some(Box::new(error));
            }
        }

        sleep(Duration::from_millis(200)).await;
    }

    Err(last_error.unwrap_or_else(|| {
        "context projection did not become ready before timeout"
            .to_string()
            .into()
    }))
}
