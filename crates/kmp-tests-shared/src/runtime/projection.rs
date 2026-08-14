use std::sync::Arc;

use async_nats::Subscriber;
use kmp_adapter_nats::NatsProjectionConsumer;
use kmp_application::{ProjectionApplicationService, RoutingProjectionWriter};
use kmp_domain::ProjectionWriter;
use kmp_testkit::{InMemoryProcessedEventStore, InMemoryProjectionCheckpointStore};
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

use crate::containers::connect_nats_with_retry;
use crate::debug::{debug_log, debug_log_value};
use crate::error::BoxError;

pub struct RunningProjectionRuntime {
    task: JoinHandle<Result<(), BoxError>>,
}

impl RunningProjectionRuntime {
    pub async fn start<G, D>(
        nats_url: &str,
        subject_prefix: &str,
        graph_writer: G,
        detail_writer: D,
    ) -> Result<Self, BoxError>
    where
        G: ProjectionWriter + Send + Sync + 'static,
        D: ProjectionWriter + Send + Sync + 'static,
    {
        debug_log_value("projection runtime nats_url", nats_url);
        let client = connect_nats_with_retry(nats_url).await?;
        let graph_subscription = client
            .subscribe(prefixed_subject(subject_prefix, "graph.node.materialized"))
            .await?;
        let relation_subscription = client
            .subscribe(prefixed_subject(
                subject_prefix,
                "graph.relation.materialized",
            ))
            .await?;
        let detail_subscription = client
            .subscribe(prefixed_subject(subject_prefix, "node.detail.materialized"))
            .await?;

        let consumer = NatsProjectionConsumer::new(subject_prefix.to_string());
        let handler = Arc::new(ProjectionApplicationService::new(
            RoutingProjectionWriter::new(graph_writer, detail_writer),
            InMemoryProcessedEventStore::default(),
            InMemoryProjectionCheckpointStore::default(),
        ));

        let task = tokio::spawn(async move {
            debug_log("projection runtime loops started");
            let graph_loop = run_subscription_loop(
                consumer.clone(),
                Arc::clone(&handler),
                graph_subscription,
                "graph.node.materialized",
            );
            let relation_loop = run_subscription_loop(
                consumer.clone(),
                Arc::clone(&handler),
                relation_subscription,
                "graph.relation.materialized",
            );
            let detail_loop = run_subscription_loop(
                consumer,
                handler,
                detail_subscription,
                "node.detail.materialized",
            );

            tokio::try_join!(graph_loop, relation_loop, detail_loop)?;
            Ok(())
        });

        Ok(Self { task })
    }

    pub async fn shutdown(self) -> Result<(), BoxError> {
        debug_log("projection runtime shutdown requested");
        self.task.abort();
        match self.task.await {
            Ok(result) => result,
            Err(join_error) if join_error.is_cancelled() => Ok(()),
            Err(join_error) => Err(Box::new(join_error)),
        }
    }
}

async fn run_subscription_loop<H>(
    consumer: NatsProjectionConsumer,
    handler: Arc<H>,
    mut subscription: Subscriber,
    subject: &'static str,
) -> Result<(), BoxError>
where
    H: kmp_application::ProjectionEventHandler + Send + Sync + 'static,
{
    while let Some(message) = subscription.next().await {
        debug_log_value("projection runtime subject", subject);
        consumer
            .consume(handler.as_ref(), subject, message.payload.as_ref())
            .await?;
    }

    Err(format!("subscription closed for {subject}").into())
}

fn prefixed_subject(subject_prefix: &str, subject: &str) -> String {
    if subject_prefix.is_empty() {
        subject.to_string()
    } else {
        format!("{subject_prefix}.{subject}")
    }
}
