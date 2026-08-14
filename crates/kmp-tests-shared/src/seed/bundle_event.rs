use std::error::Error;
use std::io;

use async_nats::Client;
use serde::{Deserialize, Serialize};

use crate::debug::debug_log_value;
use crate::seed::generic_data::{ROOT_NODE_ID, SUBJECT_PREFIX};

const CONTEXT_BUNDLE_GENERATED_SUBJECT: &str = "context.bundle.generated";

#[derive(Debug, Clone, Deserialize)]
pub struct ContextBundleGeneratedEvent {
    pub data: ContextBundleGeneratedData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextBundleGeneratedData {
    pub root_node_id: String,
    pub roles: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ContextBundleGeneratedEventPayload {
    event_id: String,
    correlation_id: String,
    causation_id: String,
    occurred_at: String,
    aggregate_id: String,
    aggregate_type: String,
    schema_version: String,
    data: ContextBundleGeneratedDataPayload,
}

#[derive(Debug, Serialize)]
struct ContextBundleGeneratedDataPayload {
    root_node_id: String,
    roles: Vec<String>,
    revision: u64,
    content_hash: String,
    projection_watermark: String,
}

pub async fn publish_context_bundle_generated_event(
    client: &Client,
    roles: &[&str],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let subject = context_bundle_generated_subject();
    debug_log_value("publishing bundle generated subject", &subject);
    client
        .publish(
            subject,
            serde_json::to_vec(&bundle_generated_event(roles))?.into(),
        )
        .await?;
    client.flush().await?;
    Ok(())
}

pub fn parse_context_bundle_generated_event(
    payload: &[u8],
) -> Result<ContextBundleGeneratedEvent, Box<dyn Error + Send + Sync>> {
    let event = serde_json::from_slice::<ContextBundleGeneratedEvent>(payload)?;
    if event.data.root_node_id.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bundle generated event missing root_node_id",
        )
        .into());
    }

    Ok(event)
}

pub fn context_bundle_generated_subject() -> String {
    format!("{SUBJECT_PREFIX}.{CONTEXT_BUNDLE_GENERATED_SUBJECT}")
}

fn bundle_generated_event(roles: &[&str]) -> ContextBundleGeneratedEventPayload {
    ContextBundleGeneratedEventPayload {
        event_id: "evt-context-bundle-agentic".to_string(),
        correlation_id: "corr-agentic-runtime".to_string(),
        causation_id: "cause-agentic-runtime".to_string(),
        occurred_at: "2026-03-10T12:05:00Z".to_string(),
        aggregate_id: ROOT_NODE_ID.to_string(),
        aggregate_type: "bundle".to_string(),
        schema_version: "v1beta1".to_string(),
        data: ContextBundleGeneratedDataPayload {
            root_node_id: ROOT_NODE_ID.to_string(),
            roles: roles.iter().map(|role| (*role).to_string()).collect(),
            revision: 42,
            content_hash: "sha256:bundle-agentic-v42".to_string(),
            projection_watermark: "evt-detail-1".to_string(),
        },
    }
}
