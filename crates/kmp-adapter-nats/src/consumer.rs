use kmp_domain::{ProjectionEventHandler, ProjectionHandlingRequest, ProjectionHandlingResult};

use super::error::NatsConsumerError;
use super::payload_decoding::decode_projection_event;
use super::subject_routing::{ProjectionSubject, stream_name, subject_prefix_pattern};

const DEFAULT_CONSUMER_NAME: &str = "context-projection";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NatsProjectionConsumer {
    subject_prefix: String,
    consumer_name: String,
}

impl NatsProjectionConsumer {
    pub fn new(subject_prefix: String) -> Self {
        Self {
            subject_prefix: subject_prefix.trim().to_string(),
            consumer_name: DEFAULT_CONSUMER_NAME.to_string(),
        }
    }

    pub fn describe(&self) -> String {
        format!(
            "nats projection consumer routing {}graph.node.materialized, {}graph.relation.materialized, and {}node.detail.materialized for {}",
            subject_prefix_pattern(&self.subject_prefix),
            subject_prefix_pattern(&self.subject_prefix),
            subject_prefix_pattern(&self.subject_prefix),
            self.consumer_name,
        )
    }

    pub async fn consume<H>(
        &self,
        handler: &H,
        subject: &str,
        payload: &[u8],
    ) -> Result<ProjectionHandlingResult, NatsConsumerError>
    where
        H: ProjectionEventHandler + Send + Sync,
    {
        let subject = ProjectionSubject::parse(&self.subject_prefix, subject)?;
        let normalized_subject = subject.as_str().to_string();
        let event = decode_projection_event(subject, payload)?;

        handler
            .handle_projection_event(ProjectionHandlingRequest {
                consumer_name: self.consumer_name.clone(),
                stream_name: stream_name(&self.subject_prefix, &self.consumer_name),
                subject: normalized_subject,
                event,
            })
            .await
            .map_err(NatsConsumerError::Application)
    }
}
