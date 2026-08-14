use kmp_proto::v1beta1::{
    context_command_service_client::ContextCommandServiceClient,
    context_query_service_client::ContextQueryServiceClient,
};

use crate::containers::{NatsContainer, Neo4jContainer, ValkeyContainer};
use crate::debug::{debug_log, debug_log_value};
use crate::error::BoxError;
use crate::fixtures::fixture::wait_for_context_ready;
use crate::ports::{SeedContext, SeedStrategy};
use crate::runtime::RunningProjectionRuntime;
use crate::server::RunningGrpcServer;

use super::TestFixture;

/// Composable fixture builder — each `with_*` call opts into an
/// infrastructure component.  OCP: new components need a new method
/// on the builder, but never change the existing build sequence.
pub struct TestFixtureBuilder {
    neo4j: bool,
    valkey: bool,
    nats: bool,
    projection_runtime: bool,
    grpc_server: bool,
    seed_strategy: Option<Box<dyn SeedStrategy>>,
    readiness_root: Option<String>,
    readiness_focus: Option<String>,
    readiness_require_detail: bool,
}

impl TestFixtureBuilder {
    pub fn new() -> Self {
        Self {
            neo4j: false,
            valkey: false,
            nats: false,
            projection_runtime: false,
            grpc_server: false,
            seed_strategy: None,
            readiness_root: None,
            readiness_focus: None,
            readiness_require_detail: true,
        }
    }

    pub fn with_neo4j(mut self) -> Self {
        self.neo4j = true;
        self
    }

    pub fn with_valkey(mut self) -> Self {
        self.valkey = true;
        self
    }

    pub fn with_nats(mut self) -> Self {
        self.nats = true;
        self
    }

    pub fn with_projection_runtime(mut self) -> Self {
        self.projection_runtime = true;
        // Projection runtime requires NATS.
        self.nats = true;
        self
    }

    pub fn with_grpc_server(mut self) -> Self {
        self.grpc_server = true;
        self
    }

    pub fn with_seed(mut self, strategy: impl SeedStrategy + 'static) -> Self {
        self.seed_strategy = Some(Box::new(strategy));
        self
    }

    pub fn with_readiness_check(
        mut self,
        root_node_id: impl Into<String>,
        focus_node_id: impl Into<String>,
    ) -> Self {
        self.readiness_root = Some(root_node_id.into());
        self.readiness_focus = Some(focus_node_id.into());
        self
    }

    pub fn require_node_detail(mut self, require: bool) -> Self {
        self.readiness_require_detail = require;
        self
    }

    /// Assembles all requested infrastructure, seeds data, and waits for readiness.
    pub async fn build(self) -> Result<TestFixture, BoxError> {
        debug_log("TestFixtureBuilder: starting build");

        // 1. Start containers.
        let neo4j = if self.neo4j {
            debug_log("starting neo4j container");
            let c = Neo4jContainer::start().await?;
            debug_log_value("neo4j endpoint", c.endpoint());
            c.clear().await?;
            debug_log("neo4j cleared");
            Some(c)
        } else {
            None
        };

        let valkey = if self.valkey {
            debug_log("starting valkey container");
            let c = ValkeyContainer::start().await?;
            debug_log_value("valkey endpoint", c.endpoint());
            Some(c)
        } else {
            None
        };

        let nats = if self.nats {
            debug_log("starting nats container");
            let c = NatsContainer::start().await?;
            debug_log_value("nats url", c.url());
            Some(c)
        } else {
            None
        };

        // 2. Create stores from containers.
        let graph_store = neo4j.as_ref().map(|n| n.graph_store()).transpose()?;
        let detail_store = valkey.as_ref().map(|v| v.detail_store()).transpose()?;
        let snapshot_store = valkey.as_ref().map(|v| v.snapshot_store()).transpose()?;

        // 3. Start projection runtime (requires NATS + stores).
        let projection_runtime = if self.projection_runtime {
            let nats_ref = nats.as_ref().expect("projection_runtime requires NATS");
            let gs = graph_store
                .clone()
                .expect("projection_runtime requires Neo4j");
            let ds = detail_store
                .clone()
                .expect("projection_runtime requires Valkey");
            let rt =
                RunningProjectionRuntime::start(&nats_ref.url(), "rehydration", gs, ds).await?;
            debug_log("projection runtime started");
            Some(rt)
        } else {
            None
        };

        // 4. Start gRPC server.
        let (server, query_client, command_client) = if self.grpc_server {
            let gs = graph_store.expect("gRPC server requires Neo4j");
            let ds = detail_store.expect("gRPC server requires Valkey");
            let ss = snapshot_store.expect("gRPC server requires Valkey");
            let srv = RunningGrpcServer::start(gs, ds, ss).await?;
            debug_log("grpc server started");
            let channel = srv.connect_channel().await?;
            debug_log("grpc channel connected");
            let qc = ContextQueryServiceClient::new(channel.clone());
            let cc = ContextCommandServiceClient::new(channel);
            (Some(srv), Some(qc), Some(cc))
        } else {
            (None, None, None)
        };

        // 5. Seed data.
        if let Some(ref strategy) = self.seed_strategy {
            let nats_client = if let Some(ref n) = nats {
                Some(n.connect().await?)
            } else {
                None
            };
            let ctx = SeedContext::new(nats_client);
            strategy.seed(&ctx).await?;
            debug_log("seed strategy completed");
        }

        // 6. Wait for readiness.
        if let (Some(root), Some(focus)) = (
            self.readiness_root.as_deref(),
            self.readiness_focus.as_deref(),
        ) && let Some(qc) = &query_client
        {
            wait_for_context_ready(qc.clone(), root, focus, self.readiness_require_detail).await?;
            debug_log("context readiness check passed");
        }

        debug_log("TestFixtureBuilder: build complete");

        Ok(TestFixture::new(
            neo4j,
            valkey,
            nats,
            projection_runtime,
            server,
            query_client,
            command_client,
        ))
    }
}

impl Default for TestFixtureBuilder {
    fn default() -> Self {
        Self::new()
    }
}
