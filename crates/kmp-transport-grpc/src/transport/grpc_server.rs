use std::fs;
use std::future::{Future, pending};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use kmp_application::{
    ApplicationError, CommandApplicationService, DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH,
    KernelMemoryApplicationService, KmpApplication, QueryApplicationService,
    RoutingProjectionWriter, UpdateContextUseCase,
};
use kmp_config::{AppConfig, GrpcTlsConfig, GrpcTlsMode};
use kmp_domain::{
    ContextEventStore, GraphNeighborhoodReader, KmpBundle, MemoryAboutIndexReader,
    NodeDetailReader, NodeRelationshipReader, ProjectionWriter, QualityMetricsObserver,
    SnapshotStore,
};
use kmp_proto::v1beta1::{
    FILE_DESCRIPTOR_SET, GetContextRequest,
    context_command_service_server::ContextCommandServiceServer,
    context_query_service_server::ContextQueryServiceServer,
    kernel_memory_service_server::KernelMemoryServiceServer,
};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};

use crate::transport::{CommandGrpcService, MemoryGrpcService, QueryGrpcService};

type ServerProjectionWriter<G, D> = RoutingProjectionWriter<Arc<G>, Arc<D>>;
type ServerCommandApplication<E, G, D> = CommandApplicationService<E, ServerProjectionWriter<G, D>>;
type ServerMemoryApplication<G, D, S, E> =
    KernelMemoryApplicationService<G, D, S, E, ServerProjectionWriter<G, D>>;

pub struct GrpcServer<G, D, S, E> {
    bind_addr: String,
    grpc_tls: GrpcTlsConfig,
    query_application: Arc<QueryApplicationService<G, D, S>>,
    command_application: Arc<ServerCommandApplication<E, G, D>>,
    quality_observer: Arc<dyn QualityMetricsObserver>,
    capability_name: &'static str,
}

impl<G, D, S, E> GrpcServer<G, D, S, E>
where
    G: GraphNeighborhoodReader
        + MemoryAboutIndexReader
        + NodeRelationshipReader
        + Send
        + Sync
        + 'static,
    G: ProjectionWriter + Send + Sync + 'static,
    D: NodeDetailReader + Send + Sync + 'static,
    D: ProjectionWriter + Send + Sync + 'static,
    S: SnapshotStore + Send + Sync + 'static,
    E: ContextEventStore + Send + Sync + 'static,
{
    pub fn new(
        config: AppConfig,
        graph_reader: G,
        detail_reader: D,
        snapshot_store: S,
        event_store: E,
        quality_observer: Arc<dyn QualityMetricsObserver>,
    ) -> Self {
        let AppConfig {
            grpc_bind,
            grpc_tls,
            ..
        } = config;
        let graph_reader = Arc::new(graph_reader);
        let detail_reader = Arc::new(detail_reader);
        let snapshot_store = Arc::new(snapshot_store);
        let event_store = Arc::new(event_store);
        let generator_version = env!("CARGO_PKG_VERSION");
        let projection_writer =
            RoutingProjectionWriter::new(Arc::clone(&graph_reader), Arc::clone(&detail_reader));
        let update_context = Arc::new(UpdateContextUseCase::new_with_projection_writer(
            event_store,
            projection_writer,
            generator_version,
        ));

        Self {
            bind_addr: grpc_bind,
            grpc_tls,
            query_application: Arc::new(QueryApplicationService::new(
                Arc::clone(&graph_reader),
                Arc::clone(&detail_reader),
                Arc::clone(&snapshot_store),
                generator_version,
            )),
            command_application: Arc::new(CommandApplicationService::new(update_context)),
            quality_observer,
            capability_name: KmpApplication::capability_name(),
        }
    }

    pub fn describe(&self) -> String {
        format!(
            "grpc transport for {} on {} (tls={})",
            self.capability_name,
            self.bind_addr,
            self.grpc_tls.mode.as_str()
        )
    }

    pub fn bootstrap_request(&self) -> GetContextRequest {
        GetContextRequest {
            root_node_id: "bootstrap-node".to_string(),
            role: "system".to_string(),
            token_budget: 4096,
            requested_scopes: Vec::new(),
            depth: DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH,
            max_tier: 0,
            rehydration_mode: 0,
        }
    }

    pub fn descriptor_set(&self) -> &'static [u8] {
        FILE_DESCRIPTOR_SET
    }

    pub fn query_service(&self) -> QueryGrpcService<G, D, S> {
        QueryGrpcService::new(
            Arc::clone(&self.query_application),
            Arc::clone(&self.quality_observer),
        )
    }

    pub fn command_service(&self) -> CommandGrpcService<E, ServerProjectionWriter<G, D>> {
        CommandGrpcService::new(Arc::clone(&self.command_application))
    }

    pub fn memory_service(&self) -> MemoryGrpcService<G, D, S, E, ServerProjectionWriter<G, D>> {
        MemoryGrpcService::new(Arc::new(ServerMemoryApplication::new(
            Arc::clone(&self.query_application),
            Arc::clone(&self.command_application),
        )))
    }

    pub fn query_application(&self) -> Arc<QueryApplicationService<G, D, S>> {
        Arc::clone(&self.query_application)
    }

    pub fn command_application(&self) -> Arc<ServerCommandApplication<E, G, D>> {
        Arc::clone(&self.command_application)
    }

    pub async fn warmup_bundle(&self) -> Result<KmpBundle, ApplicationError> {
        self.query_application.warmup_bundle().await
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let bind_addr: SocketAddr = self.bind_addr.parse()?;
        let listener = TcpListener::bind(bind_addr).await?;

        self.serve_with_listener_shutdown(listener, pending()).await
    }

    pub async fn serve_with_listener_shutdown<F>(
        self,
        listener: TcpListener,
        shutdown: F,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.transport_builder()?
            .add_service(ContextQueryServiceServer::new(self.query_service()))
            .add_service(ContextCommandServiceServer::new(self.command_service()))
            .add_service(KernelMemoryServiceServer::new(self.memory_service()))
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), shutdown)
            .await?;

        Ok(())
    }

    fn transport_builder(&self) -> Result<Server, Box<dyn std::error::Error + Send + Sync>> {
        let builder = Server::builder();

        match self.grpc_tls.mode {
            GrpcTlsMode::Disabled => Ok(builder),
            GrpcTlsMode::Server | GrpcTlsMode::Mutual => builder
                .tls_config(load_server_tls_config(&self.grpc_tls)?)
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>),
        }
    }
}

fn load_server_tls_config(
    grpc_tls: &GrpcTlsConfig,
) -> Result<ServerTlsConfig, Box<dyn std::error::Error + Send + Sync>> {
    let cert_pem = read_required_pem(grpc_tls.cert_path.as_deref(), "KMP_GRPC_TLS_CERT_PATH")?;
    let key_pem = read_required_pem(grpc_tls.key_path.as_deref(), "KMP_GRPC_TLS_KEY_PATH")?;
    let identity = Identity::from_pem(cert_pem, key_pem);
    let mut tls_config = ServerTlsConfig::new().identity(identity);

    if grpc_tls.mode == GrpcTlsMode::Mutual {
        let client_ca_pem = read_required_pem(
            grpc_tls.client_ca_path.as_deref(),
            "KMP_GRPC_TLS_CLIENT_CA_PATH",
        )?;
        tls_config = tls_config.client_ca_root(Certificate::from_pem(client_ca_pem));
    }

    Ok(tls_config)
}

fn read_required_pem(
    path: Option<&Path>,
    env_name: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let path = path.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{env_name} must be configured when gRPC TLS is enabled"),
        )
    })?;

    fs::read(path).map_err(|error| {
        Box::new(std::io::Error::new(
            error.kind(),
            format!(
                "failed to read {} from {}: {error}",
                env_name,
                path.display()
            ),
        )) as Box<dyn std::error::Error + Send + Sync>
    })
}
