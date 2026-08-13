use std::sync::Arc;

use async_nats::jetstream;
use async_nats::jetstream::message::AckKind;
use tokio_stream::StreamExt;

use kmp_domain::ProjectionEventHandler;

use crate::consumer::NatsProjectionConsumer;
use crate::runtime::connect_options::{NatsClientTlsConfig, connect_nats_client};
use crate::runtime::projection_stream::{
    DETAIL_SUBJECT, GRAPH_SUBJECT, RELATION_SUBJECT, ensure_detail_consumer, ensure_graph_consumer,
    ensure_relation_consumer, ensure_stream,
};
use crate::runtime::runtime_error::NatsRuntimeError;

#[derive(Debug)]
pub struct NatsProjectionRuntime<H> {
    consumer: NatsProjectionConsumer,
    handler: Arc<H>,
    graph_consumer: jetstream::consumer::PullConsumer,
    relation_consumer: jetstream::consumer::PullConsumer,
    detail_consumer: jetstream::consumer::PullConsumer,
}

impl<H> NatsProjectionRuntime<H>
where
    H: ProjectionEventHandler + Send + Sync + 'static,
{
    pub async fn connect(
        url: &str,
        tls: &NatsClientTlsConfig,
        subject_prefix: &str,
        handler: H,
    ) -> Result<Self, NatsRuntimeError> {
        let client = connect_nats_client(url, tls).await?;
        let jetstream = jetstream::new(client);
        let stream = ensure_stream(&jetstream, subject_prefix).await?;

        Ok(Self {
            consumer: NatsProjectionConsumer::new(subject_prefix.to_string()),
            handler: Arc::new(handler),
            graph_consumer: ensure_graph_consumer(&stream, subject_prefix).await?,
            relation_consumer: ensure_relation_consumer(&stream, subject_prefix).await?,
            detail_consumer: ensure_detail_consumer(&stream, subject_prefix).await?,
        })
    }

    pub fn describe(&self) -> String {
        format!(
            "nats projection runtime via JetStream with {}",
            self.consumer.describe()
        )
    }

    pub async fn run(self) -> Result<(), NatsRuntimeError> {
        let graph_consumer = self.consumer.clone();
        let relation_consumer = self.consumer.clone();
        let graph_handler = Arc::clone(&self.handler);
        let relation_handler = Arc::clone(&self.handler);

        let graph_task = run_subject_loop(
            graph_consumer,
            graph_handler,
            self.graph_consumer,
            GRAPH_SUBJECT,
        );
        let detail_task = run_subject_loop(
            self.consumer,
            self.handler,
            self.detail_consumer,
            DETAIL_SUBJECT,
        );
        let relation_task = run_subject_loop(
            relation_consumer,
            relation_handler,
            self.relation_consumer,
            RELATION_SUBJECT,
        );

        tokio::try_join!(graph_task, relation_task, detail_task)?;
        Ok(())
    }
}

async fn run_subject_loop<H>(
    consumer: NatsProjectionConsumer,
    handler: Arc<H>,
    pull_consumer: jetstream::consumer::PullConsumer,
    subject: &'static str,
) -> Result<(), NatsRuntimeError>
where
    H: ProjectionEventHandler + Send + Sync + 'static,
{
    let mut messages = pull_consumer
        .messages()
        .await
        .map_err(|error| NatsRuntimeError::Subscription(error.to_string()))?;

    while let Some(message) = messages.next().await {
        let message = message.map_err(|error| NatsRuntimeError::Message(error.to_string()))?;

        let receive_time = std::time::Instant::now();
        match consumer
            .consume(handler.as_ref(), subject, message.payload.as_ref())
            .await
        {
            Ok(_) => {
                let processing_secs = receive_time.elapsed().as_secs_f64();
                opentelemetry::global::meter("kmp")
                    .f64_histogram("rehydration.projection.lag")
                    .build()
                    .record(
                        processing_secs,
                        &[opentelemetry::KeyValue::new("subject", subject.to_string())],
                    );
                message
                    .ack()
                    .await
                    .map_err(|error| NatsRuntimeError::Message(error.to_string()))?;
            }
            Err(error) => {
                message
                    .ack_with(AckKind::Nak(None))
                    .await
                    .map_err(|nak_error| NatsRuntimeError::Message(nak_error.to_string()))?;
                return Err(NatsRuntimeError::Consumer(error));
            }
        }
    }

    Err(NatsRuntimeError::Subscription(format!(
        "subscription closed for {subject}"
    )))
}
