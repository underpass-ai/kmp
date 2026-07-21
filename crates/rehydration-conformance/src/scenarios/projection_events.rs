//! Event-driven projection semantics: dedup by `event_id` and replay safety,
//! driving `ProjectionApplicationService::handle_projection_event` in-process
//! exactly as the infrastructure NATS runtime does.

use std::collections::BTreeMap;

use rehydration_domain::{
    GraphNeighborhoodReader, GraphNodeMaterializedData, GraphNodeMaterializedEvent,
    NodeDetailMaterializedData, NodeDetailMaterializedEvent, NodeDetailReader, ProjectionEnvelope,
    ProjectionEvent, ProjectionEventHandler, ProjectionHandlingRequest,
};

use crate::ConformanceBackendFactory;

const CONSUMER: &str = "conformance-consumer";
const STREAM: &str = "conformance-stream";

fn envelope(event_id: &str) -> ProjectionEnvelope {
    ProjectionEnvelope {
        event_id: event_id.to_string(),
        correlation_id: "conf-correlation".to_string(),
        causation_id: "conf-causation".to_string(),
        occurred_at: "2026-07-21T00:00:00Z".to_string(),
        aggregate_id: "conf:aggregate".to_string(),
        aggregate_type: "projection".to_string(),
        schema_version: "v1beta1".to_string(),
    }
}

fn node_event(event_id: &str, node_id: &str, title: &str) -> ProjectionHandlingRequest {
    ProjectionHandlingRequest {
        consumer_name: CONSUMER.to_string(),
        stream_name: STREAM.to_string(),
        subject: "graph.node.materialized".to_string(),
        event: ProjectionEvent::GraphNodeMaterialized(GraphNodeMaterializedEvent {
            envelope: envelope(event_id),
            data: GraphNodeMaterializedData {
                node_id: node_id.to_string(),
                node_kind: "claim".to_string(),
                title: title.to_string(),
                summary: format!("Summary for {node_id}"),
                status: "ACTIVE".to_string(),
                labels: vec!["conformance".to_string()],
                properties: BTreeMap::new(),
                related_nodes: Vec::new(),
                source_kind: None,
                source_agent: None,
                observed_at: None,
            },
        }),
    }
}

fn detail_event(event_id: &str, node_id: &str, detail: &str) -> ProjectionHandlingRequest {
    ProjectionHandlingRequest {
        consumer_name: CONSUMER.to_string(),
        stream_name: STREAM.to_string(),
        subject: "node.detail.materialized".to_string(),
        event: ProjectionEvent::NodeDetailMaterialized(NodeDetailMaterializedEvent {
            envelope: envelope(event_id),
            data: NodeDetailMaterializedData {
                node_id: node_id.to_string(),
                detail: detail.to_string(),
                content_hash: format!("hash-{event_id}"),
                revision: 1,
            },
        }),
    }
}

pub async fn projection_events_dedup_by_event_id(factory: &impl ConformanceBackendFactory) {
    let backend = factory.fresh().await;
    let service = backend.projection_service();

    let first = service
        .handle_projection_event(node_event("evt-1", "conf:event-node", "From event"))
        .await
        .expect("first event should apply");
    assert!(!first.duplicate, "first delivery must not be a duplicate");
    assert!(
        first.applied_mutations > 0,
        "first delivery must apply mutations"
    );

    let second = service
        .handle_projection_event(node_event("evt-1", "conf:event-node", "Replayed delivery"))
        .await
        .expect("duplicate event should be handled");
    assert!(
        second.duplicate,
        "same event_id for the same consumer must dedup"
    );
    assert_eq!(
        second.applied_mutations, 0,
        "duplicate delivery must not reapply mutations"
    );

    let neighborhood = backend
        .graph
        .load_neighborhood("conf:event-node", 1)
        .await
        .expect("read should succeed")
        .expect("event-projected node must be readable");
    assert_eq!(
        neighborhood.root.title, "From event",
        "duplicate delivery must not overwrite the first application"
    );
}

pub async fn projection_event_replay_converges_to_same_state(
    factory: &impl ConformanceBackendFactory,
) {
    let backend = factory.fresh().await;
    let service = backend.projection_service();

    service
        .handle_projection_event(node_event("evt-node-1", "conf:replayed", "Replayed"))
        .await
        .expect("node event should apply");
    service
        .handle_projection_event(detail_event("evt-detail-1", "conf:replayed", "same detail"))
        .await
        .expect("detail event should apply");

    let before = backend
        .detail
        .load_node_detail("conf:replayed")
        .await
        .expect("detail read should succeed")
        .expect("detail must exist");

    // Replay with fresh event ids (e.g. a projection rebuild) must converge
    // to the same observable state, not duplicate or corrupt it.
    let replayed_node = service
        .handle_projection_event(node_event("evt-node-2", "conf:replayed", "Replayed"))
        .await
        .expect("replayed node event should apply");
    assert!(!replayed_node.duplicate);
    let replayed_detail = service
        .handle_projection_event(detail_event("evt-detail-2", "conf:replayed", "same detail"))
        .await
        .expect("replayed detail event should apply");
    assert!(!replayed_detail.duplicate);

    let after_neighborhood = backend
        .graph
        .load_neighborhood("conf:replayed", 1)
        .await
        .expect("read should succeed")
        .expect("node must exist");
    assert_eq!(after_neighborhood.root.title, "Replayed");
    assert!(
        after_neighborhood.neighbors.is_empty(),
        "replay must not invent neighbors"
    );

    let after = backend
        .detail
        .load_node_detail("conf:replayed")
        .await
        .expect("detail read should succeed")
        .expect("detail must exist");
    assert_eq!(
        after.detail, before.detail,
        "replayed detail must converge to the same text"
    );
    assert_eq!(after.revision, before.revision);
}
