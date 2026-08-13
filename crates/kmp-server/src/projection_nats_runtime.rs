use std::error::Error;

use kmp_adapter_nats::NatsProjectionRuntime;
use kmp_adapter_valkey::{ValkeyProcessedEventStore, ValkeyProjectionCheckpointStore};
use kmp_application::{ProjectionApplicationService, RoutingProjectionWriter};
use kmp_config::ProjectionRuntimeConfig;
use kmp_domain::ProjectionWriter;

use crate::nats_tls::adapter_nats_tls_config;

pub async fn connect_projection_runtime<G, D>(
    config: &ProjectionRuntimeConfig,
    subject_prefix: &str,
    graph_writer: G,
    detail_writer: D,
) -> Result<
    NatsProjectionRuntime<
        ProjectionApplicationService<
            RoutingProjectionWriter<G, D>,
            ValkeyProcessedEventStore,
            ValkeyProjectionCheckpointStore,
        >,
    >,
    Box<dyn Error + Send + Sync>,
>
where
    G: ProjectionWriter + Send + Sync + 'static,
    D: ProjectionWriter + Send + Sync + 'static,
{
    let processed_event_store = ValkeyProcessedEventStore::new(config.runtime_state_uri.clone())?;
    let checkpoint_store = ValkeyProjectionCheckpointStore::new(config.runtime_state_uri.clone())?;
    let handler = ProjectionApplicationService::new(
        RoutingProjectionWriter::new(graph_writer, detail_writer),
        processed_event_store,
        checkpoint_store,
    );

    NatsProjectionRuntime::connect(
        &config.nats_url,
        &adapter_nats_tls_config(&config.nats_tls),
        subject_prefix,
        handler,
    )
    .await
    .map_err(|error| Box::new(error) as Box<dyn Error + Send + Sync>)
}

#[cfg(test)]
mod tests {
    use kmp_config::{NatsTlsConfig, ProjectionRuntimeConfig};
    use kmp_domain::{PortError, ProjectionMutation, ProjectionWriter};

    use super::connect_projection_runtime;

    #[derive(Debug, Clone, Copy)]
    struct NoopProjectionWriter;

    impl ProjectionWriter for NoopProjectionWriter {
        async fn apply_mutations(
            &self,
            _mutations: Vec<ProjectionMutation>,
        ) -> Result<(), PortError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn projection_runtime_surfaces_connection_errors() {
        let error = match connect_projection_runtime(
            &ProjectionRuntimeConfig {
                nats_url: "nats://127.0.0.1:1".to_string(),
                runtime_state_uri: "redis://127.0.0.1:6379".to_string(),
                nats_tls: NatsTlsConfig::disabled(),
            },
            "rehydration",
            NoopProjectionWriter,
            NoopProjectionWriter,
        )
        .await
        {
            Ok(_) => panic!("invalid nats endpoint should fail during startup"),
            Err(error) => error,
        };

        assert!(
            error.to_string().contains("connection"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn projection_runtime_rejects_invalid_runtime_state_uri() {
        let error = match connect_projection_runtime(
            &ProjectionRuntimeConfig {
                nats_url: "nats://127.0.0.1:4222".to_string(),
                runtime_state_uri: "http://127.0.0.1:6379".to_string(),
                nats_tls: NatsTlsConfig::disabled(),
            },
            "rehydration",
            NoopProjectionWriter,
            NoopProjectionWriter,
        )
        .await
        {
            Ok(_) => panic!("invalid valkey uri should fail before connecting to nats"),
            Err(error) => error,
        };

        assert!(
            error
                .to_string()
                .contains("unsupported processed event scheme"),
            "unexpected error: {error}"
        );
    }
}
