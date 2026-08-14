mod nats_tls;
mod projection_nats_runtime;

use std::process::ExitCode;
use std::sync::{Arc, Once};

use kmp_adapter_nats::{NatsContextEventStore, NatsProjectionConsumer, connect_nats_client};
use kmp_adapter_neo4j::Neo4jProjectionReader;
use kmp_adapter_valkey::{ValkeyContextEventStore, ValkeyNodeDetailStore, ValkeySnapshotStore};
use kmp_config::{AppConfig, ProjectionRuntimeConfig};
use kmp_domain::ContextEventStore;
use kmp_observability::quality_observers::{
    CompositeQualityObserver, OTelQualityObserver, TracingQualityObserver,
};
use kmp_observability::{init_observability, shutdown_observability};
use kmp_transport_grpc::GrpcServer;

use crate::nats_tls::adapter_nats_tls_config;
use crate::projection_nats_runtime::connect_projection_runtime;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "kmp-server failed");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    install_rustls_crypto_provider();
    let config = AppConfig::try_from_env()?;
    let projection_runtime_config = ProjectionRuntimeConfig::try_from_env()?;
    let otel_guard = init_observability(&config.service_name);

    let meter = opentelemetry::global::meter("kmp");
    let quality_observer: Arc<dyn kmp_domain::QualityMetricsObserver> =
        Arc::new(CompositeQualityObserver::new(vec![
            Box::new(OTelQualityObserver::new(&meter)),
            Box::new(TracingQualityObserver),
        ]));

    let graph_reader = Neo4jProjectionReader::new(config.graph_uri.clone())?;
    let detail_reader = ValkeyNodeDetailStore::new(config.detail_uri.clone())?;
    let snapshot_store = ValkeySnapshotStore::new(config.snapshot_uri.clone())?;

    let event_store_backend =
        std::env::var("KMP_EVENT_STORE_BACKEND").unwrap_or_else(|_| "valkey".to_string());

    match event_store_backend.as_str() {
        "nats" => {
            let nats_tls = adapter_nats_tls_config(&projection_runtime_config.nats_tls);
            let nats_client =
                connect_nats_client(&projection_runtime_config.nats_url, &nats_tls).await?;
            let event_store =
                NatsContextEventStore::new(nats_client, &config.events_subject_prefix).await?;
            run_server(ServerContext {
                config,
                graph_reader,
                detail_reader,
                snapshot_store,
                event_store,
                quality_observer: quality_observer.clone(),
                projection_config: projection_runtime_config,
            })
            .await?;
        }
        _ => {
            let event_store = ValkeyContextEventStore::new(config.snapshot_uri.clone())?;
            run_server(ServerContext {
                config,
                graph_reader,
                detail_reader,
                snapshot_store,
                event_store,
                quality_observer: quality_observer.clone(),
                projection_config: projection_runtime_config,
            })
            .await?;
        }
    }

    shutdown_observability(otel_guard);
    Ok(())
}

struct ServerContext<E, G, D> {
    config: AppConfig,
    graph_reader: G,
    detail_reader: D,
    snapshot_store: ValkeySnapshotStore,
    event_store: E,
    quality_observer: Arc<dyn kmp_domain::QualityMetricsObserver>,
    projection_config: ProjectionRuntimeConfig,
}

async fn run_server<E, G, D>(
    ctx: ServerContext<E, G, D>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    E: ContextEventStore + Send + Sync + 'static + std::fmt::Debug,
    G: kmp_domain::GraphNeighborhoodReader
        + kmp_domain::MemoryAboutIndexReader
        + kmp_domain::NodeRelationshipReader
        + kmp_domain::ProjectionWriter
        + Send
        + Sync
        + Clone
        + 'static,
    D: kmp_domain::NodeDetailReader + kmp_domain::ProjectionWriter + Send + Sync + Clone + 'static,
{
    let grpc_server = GrpcServer::new(
        ctx.config.clone(),
        ctx.graph_reader.clone(),
        ctx.detail_reader.clone(),
        ctx.snapshot_store,
        ctx.event_store,
        ctx.quality_observer,
    );
    let events_consumer = NatsProjectionConsumer::new(ctx.config.events_subject_prefix.clone());
    let projection_runtime = connect_projection_runtime(
        &ctx.projection_config,
        &ctx.config.events_subject_prefix,
        ctx.graph_reader,
        ctx.detail_reader,
    )
    .await?;

    tracing::info!(grpc = %grpc_server.describe(), "server ready");
    tracing::info!(events = %events_consumer.describe(), "events ready");
    tracing::info!(projection = %projection_runtime.describe(), "projection ready");
    tracing::info!(
        role = %grpc_server.bootstrap_request().role,
        "query bootstrap"
    );

    match grpc_server.warmup_bundle().await {
        Ok(_) => tracing::info!("warmup bundle loaded"),
        Err(error) => tracing::warn!(%error, "warmup bundle skipped (node may not exist yet)"),
    }

    tokio::try_join!(async { grpc_server.run().await }, async {
        projection_runtime
            .run()
            .await
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
    })?;

    Ok(())
}

fn install_rustls_crypto_provider() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}
