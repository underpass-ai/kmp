#![allow(dead_code)]

use std::collections::BTreeMap;
use std::error::Error;

use async_nats::Client;
use kmp_application::{
    GraphNodeMaterializedData, GraphNodeMaterializedEvent, NodeDetailMaterializedData,
    NodeDetailMaterializedEvent, ProjectionEnvelope, RelatedNodeExplanationData,
    RelatedNodeReference,
};
use kmp_domain::RelationSemanticClass;

use crate::debug::debug_log_value;

pub const ROOT_NODE_ID: &str = "node:workspace:billing-api";
pub const ROOT_NODE_KIND: &str = "workspace";
pub const ROOT_TITLE: &str = "Billing API";
pub const ROOT_SUMMARY: &str = "Rust service responsible for payment processing flows.";
pub const ROOT_STATUS: &str = "active";

pub const FOCUS_NODE_ID: &str = "node:work_item:retry-handler";
pub const FOCUS_NODE_KIND: &str = "work_item";
pub const FOCUS_TITLE: &str = "Retry failed settlement calls";
pub const FOCUS_SUMMARY: &str = "Implements bounded retries with backoff.";
pub const FOCUS_STATUS: &str = "in_progress";
pub const FOCUS_DETAIL: &str =
    "The retry handler wraps settlement calls with capped exponential backoff and alerting hooks.";

pub const DEPENDENCY_NODE_ID: &str = "node:dependency:payments-sdk";
pub const DEPENDENCY_NODE_KIND: &str = "dependency";
pub const DEPENDENCY_TITLE: &str = "Payments SDK";
pub const DEPENDENCY_SUMMARY: &str = "Shared library for settlement and charge flows.";
pub const DEPENDENCY_STATUS: &str = "stable";

pub const CONTAINS_RELATION: &str = "contains";
pub const DEPENDS_ON_RELATION: &str = "depends_on";
pub const SUBJECT_PREFIX: &str = "rehydration";

type ProjectionMessagesResult = Result<Vec<(String, Vec<u8>)>, Box<dyn Error + Send + Sync>>;

pub async fn publish_projection_events(
    client: &Client,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for (subject, payload) in projection_messages()? {
        debug_log_value("publishing projection subject", &subject);
        client.publish(subject, payload.into()).await?;
    }
    client.flush().await?;
    Ok(())
}

fn projection_messages() -> ProjectionMessagesResult {
    Ok(vec![
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&root_node_event())?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&focus_node_event())?,
        ),
        (
            subject("graph.node.materialized"),
            serde_json::to_vec(&dependency_node_event())?,
        ),
        (
            subject("node.detail.materialized"),
            serde_json::to_vec(&focus_detail_event())?,
        ),
    ])
}

fn subject(suffix: &str) -> String {
    format!("{SUBJECT_PREFIX}.{suffix}")
}

fn root_node_event() -> GraphNodeMaterializedEvent {
    GraphNodeMaterializedEvent {
        envelope: base_envelope("evt-root-1", ROOT_NODE_ID, "node"),
        data: GraphNodeMaterializedData {
            node_id: ROOT_NODE_ID.to_string(),
            node_kind: ROOT_NODE_KIND.to_string(),
            title: ROOT_TITLE.to_string(),
            summary: ROOT_SUMMARY.to_string(),
            status: ROOT_STATUS.to_string(),
            labels: vec!["rust".to_string(), "backend".to_string()],
            properties: BTreeMap::from([
                ("language".to_string(), "rust".to_string()),
                ("service".to_string(), "billing-api".to_string()),
            ]),
            related_nodes: vec![
                RelatedNodeReference {
                    node_id: FOCUS_NODE_ID.to_string(),
                    relation_type: CONTAINS_RELATION.to_string(),
                    explanation: RelatedNodeExplanationData {
                        semantic_class: RelationSemanticClass::Causal,
                        rationale: Some(
                            "the retry handler is the primary work item inside this workspace"
                                .to_string(),
                        ),
                        motivation: None,
                        method: None,
                        decision_id: None,
                        caused_by_node_id: None,
                        evidence: None,
                        confidence: None,
                        sequence: Some(1),
                    },
                },
                RelatedNodeReference {
                    node_id: DEPENDENCY_NODE_ID.to_string(),
                    relation_type: DEPENDS_ON_RELATION.to_string(),
                    explanation: RelatedNodeExplanationData {
                        semantic_class: RelationSemanticClass::Constraint,
                        rationale: Some(
                            "settlement retries rely on the shared payments SDK".to_string(),
                        ),
                        motivation: None,
                        method: None,
                        decision_id: None,
                        caused_by_node_id: None,
                        evidence: None,
                        confidence: None,
                        sequence: Some(2),
                    },
                },
            ],
            source_kind: None,
            source_agent: None,
            observed_at: None,
        },
    }
}

fn focus_node_event() -> GraphNodeMaterializedEvent {
    GraphNodeMaterializedEvent {
        envelope: base_envelope("evt-focus-1", FOCUS_NODE_ID, "node"),
        data: GraphNodeMaterializedData {
            node_id: FOCUS_NODE_ID.to_string(),
            node_kind: FOCUS_NODE_KIND.to_string(),
            title: FOCUS_TITLE.to_string(),
            summary: FOCUS_SUMMARY.to_string(),
            status: FOCUS_STATUS.to_string(),
            labels: vec!["payments".to_string(), "resilience".to_string()],
            properties: BTreeMap::from([("owner".to_string(), "team-payments".to_string())]),
            related_nodes: Vec::new(),
            source_kind: None,
            source_agent: None,
            observed_at: None,
        },
    }
}

fn dependency_node_event() -> GraphNodeMaterializedEvent {
    GraphNodeMaterializedEvent {
        envelope: base_envelope("evt-dependency-1", DEPENDENCY_NODE_ID, "node"),
        data: GraphNodeMaterializedData {
            node_id: DEPENDENCY_NODE_ID.to_string(),
            node_kind: DEPENDENCY_NODE_KIND.to_string(),
            title: DEPENDENCY_TITLE.to_string(),
            summary: DEPENDENCY_SUMMARY.to_string(),
            status: DEPENDENCY_STATUS.to_string(),
            labels: vec!["library".to_string()],
            properties: BTreeMap::from([("version".to_string(), "3.4.1".to_string())]),
            related_nodes: Vec::new(),
            source_kind: None,
            source_agent: None,
            observed_at: None,
        },
    }
}

fn focus_detail_event() -> NodeDetailMaterializedEvent {
    NodeDetailMaterializedEvent {
        envelope: base_envelope("evt-detail-1", FOCUS_NODE_ID, "node_detail"),
        data: NodeDetailMaterializedData {
            node_id: FOCUS_NODE_ID.to_string(),
            detail: FOCUS_DETAIL.to_string(),
            content_hash: "sha256:retry-handler-v3".to_string(),
            revision: 7,
        },
    }
}

fn base_envelope(event_id: &str, aggregate_id: &str, aggregate_type: &str) -> ProjectionEnvelope {
    ProjectionEnvelope {
        event_id: event_id.to_string(),
        correlation_id: "corr-agentic-runtime".to_string(),
        causation_id: "cause-agentic-runtime".to_string(),
        occurred_at: "2026-03-10T12:00:00Z".to_string(),
        aggregate_id: aggregate_id.to_string(),
        aggregate_type: aggregate_type.to_string(),
        schema_version: "v1beta1".to_string(),
    }
}
