use std::sync::Arc;

use kmp_domain::{
    PortError, ProjectionEvent, ProjectionEventHandler, ProjectionHandlingRequest,
    ProjectionHandlingResult,
};
use serde_json::json;
use tokio::sync::Mutex;

use crate::{NatsConsumerError, NatsProjectionConsumer};

#[derive(Debug, Default)]
struct RecordingHandler {
    requests: Arc<Mutex<Vec<ProjectionHandlingRequest>>>,
}

impl RecordingHandler {
    async fn requests(&self) -> Vec<ProjectionHandlingRequest> {
        self.requests.lock().await.clone()
    }
}

impl ProjectionEventHandler for RecordingHandler {
    async fn handle_projection_event(
        &self,
        request: ProjectionHandlingRequest,
    ) -> Result<ProjectionHandlingResult, PortError> {
        self.requests.lock().await.push(request.clone());
        Ok(ProjectionHandlingResult {
            event_id: request.event.event_id().to_string(),
            subject: request.subject,
            duplicate: false,
            applied_mutations: 1,
            checkpoint: None,
        })
    }
}

#[test]
fn describe_mentions_subject_prefix() {
    let consumer = NatsProjectionConsumer::new("rehydration".to_string());
    assert!(consumer.describe().contains("rehydration"));
}

#[tokio::test]
async fn consume_routes_prefixed_graph_node_subject() {
    let consumer = NatsProjectionConsumer::new("rehydration".to_string());
    let handler = RecordingHandler::default();
    let payload = json!({
        "event_id": "evt-1",
        "correlation_id": "corr-1",
        "causation_id": "cmd-1",
        "occurred_at": "2026-03-07T20:00:00Z",
        "aggregate_id": "node-123",
        "aggregate_type": "node",
        "schema_version": "v1beta1",
        "data": {
            "node_id": "node-123",
            "node_kind": "capability",
            "title": "Projection consumer foundation",
            "summary": "Node centric projection input",
            "status": "ACTIVE",
            "labels": ["projection"],
            "properties": {"phase": "build"},
            "related_nodes": [
                {
                    "node_id": "node-122",
                    "relation_type": "depends_on",
                    "explanation": {
                        "semantic_class": "constraint",
                        "sequence": 1
                    }
                }
            ]
        }
    });

    let result = consumer
        .consume(
            &handler,
            "rehydration.graph.node.materialized",
            payload.to_string().as_bytes(),
        )
        .await
        .expect("graph node event should be routed");

    assert_eq!(result.subject, "graph.node.materialized");
    let requests = handler.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].stream_name, "rehydration.events");
    match &requests[0].event {
        ProjectionEvent::GraphNodeMaterialized(event) => {
            assert_eq!(event.data.node_id, "node-123");
        }
        event => panic!("unexpected event: {event:?}"),
    }
}

#[tokio::test]
async fn consume_routes_node_detail_subject_without_prefix() {
    let consumer = NatsProjectionConsumer::new(String::new());
    let handler = RecordingHandler::default();
    let payload = json!({
        "event_id": "evt-2",
        "correlation_id": "corr-2",
        "causation_id": "cmd-2",
        "occurred_at": "2026-03-07T20:01:00Z",
        "aggregate_id": "node-123",
        "aggregate_type": "node",
        "schema_version": "v1beta1",
        "data": {
            "node_id": "node-123",
            "detail": "Expanded node detail",
            "content_hash": "hash-123",
            "revision": 2
        }
    });

    consumer
        .consume(
            &handler,
            "node.detail.materialized",
            payload.to_string().as_bytes(),
        )
        .await
        .expect("node detail event should be routed");

    let requests = handler.requests().await;
    match &requests[0].event {
        ProjectionEvent::NodeDetailMaterialized(event) => {
            assert_eq!(event.data.content_hash, "hash-123");
        }
        event => panic!("unexpected event: {event:?}"),
    }
}

#[tokio::test]
async fn consume_routes_graph_relation_subject_without_prefix() {
    let consumer = NatsProjectionConsumer::new(String::new());
    let handler = RecordingHandler::default();
    let payload = json!({
        "event_id": "evt-2b",
        "correlation_id": "corr-2b",
        "causation_id": "cmd-2b",
        "occurred_at": "2026-03-07T20:01:30Z",
        "aggregate_id": "relation:decision|addresses|finding",
        "aggregate_type": "node_relation",
        "schema_version": "v1beta1",
        "data": {
            "source_node_id": "decision-123",
            "target_node_id": "finding-123",
            "relation_type": "addresses",
            "explanation": {
                "semantic_class": "causal",
                "decision_id": "decision-123",
                "sequence": 2
            }
        }
    });

    consumer
        .consume(
            &handler,
            "graph.relation.materialized",
            payload.to_string().as_bytes(),
        )
        .await
        .expect("graph relation event should be routed");

    let requests = handler.requests().await;
    match &requests[0].event {
        ProjectionEvent::GraphRelationMaterialized(event) => {
            assert_eq!(event.data.source_node_id, "decision-123");
            assert_eq!(event.data.target_node_id, "finding-123");
            assert_eq!(event.data.relation_type, "addresses");
        }
        event => panic!("unexpected event: {event:?}"),
    }
}

#[tokio::test]
async fn consume_rejects_unsupported_subject() {
    let consumer = NatsProjectionConsumer::new("rehydration".to_string());
    let handler = RecordingHandler::default();
    let error = consumer
        .consume(&handler, "rehydration.unknown.subject", b"{}")
        .await
        .expect_err("unsupported subjects must fail");

    assert!(matches!(error, NatsConsumerError::UnsupportedSubject(_)));
}

#[tokio::test]
async fn consume_rejects_invalid_json_payload() {
    let consumer = NatsProjectionConsumer::new(String::new());
    let handler = RecordingHandler::default();
    let error = consumer
        .consume(&handler, "node.detail.materialized", b"not-json")
        .await
        .expect_err("invalid payload must fail");

    assert!(matches!(error, NatsConsumerError::InvalidPayload(_)));
}

#[tokio::test]
async fn consume_rejects_graph_nodes_missing_relation_explanation() {
    let consumer = NatsProjectionConsumer::new("rehydration".to_string());
    let handler = RecordingHandler::default();
    let payload = json!({
        "event_id": "evt-3",
        "correlation_id": "corr-3",
        "causation_id": "cmd-3",
        "occurred_at": "2026-03-07T20:02:00Z",
        "aggregate_id": "node-123",
        "aggregate_type": "node",
        "schema_version": "v1beta1",
        "data": {
            "node_id": "node-123",
            "node_kind": "capability",
            "title": "Projection consumer foundation",
            "summary": "Node centric projection input",
            "status": "ACTIVE",
            "labels": ["projection"],
            "properties": {"phase": "build"},
            "related_nodes": [
                {"node_id": "node-122", "relation_type": "depends_on"}
            ]
        }
    });

    let error = consumer
        .consume(
            &handler,
            "rehydration.graph.node.materialized",
            payload.to_string().as_bytes(),
        )
        .await
        .expect_err("events without relationship explanation must fail");

    assert!(matches!(
        error,
        NatsConsumerError::InvalidPayload(message)
            if message.contains("missing field `explanation`")
    ));
}
