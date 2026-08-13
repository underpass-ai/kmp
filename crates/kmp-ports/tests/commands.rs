use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::SystemTime;

use kmp_domain::{BundleMetadata, BundleNode, BundleNodeDetail, CaseId, Role};
use kmp_ports::commands::{
    ProcessedEventStore, ProjectionCheckpoint, ProjectionCheckpointStore, ProjectionMutation,
    ProjectionWriter, SnapshotSaveOptions, SnapshotStore,
};
use kmp_ports::{KmpBundle, NodeDetailProjection, NodeProjection, PortError};

struct Writer;

impl ProjectionWriter for Writer {
    async fn apply_mutations(&self, mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        assert_eq!(mutations.len(), 1);
        Ok(())
    }
}

struct EventStore;

impl ProcessedEventStore for EventStore {
    async fn has_processed(&self, _consumer_name: &str, event_id: &str) -> Result<bool, PortError> {
        Ok(event_id == "evt-1")
    }

    async fn record_processed(
        &self,
        _consumer_name: &str,
        _event_id: &str,
    ) -> Result<(), PortError> {
        Ok(())
    }
}

struct CheckpointStore;

impl ProjectionCheckpointStore for CheckpointStore {
    async fn load_checkpoint(
        &self,
        consumer_name: &str,
        stream_name: &str,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        Ok(Some(ProjectionCheckpoint {
            consumer_name: consumer_name.to_string(),
            stream_name: stream_name.to_string(),
            last_subject: "graph.node.materialized".to_string(),
            last_event_id: "evt-1".to_string(),
            last_correlation_id: "corr-1".to_string(),
            last_occurred_at: "2026-03-07T00:00:00Z".to_string(),
            processed_events: 1,
            updated_at: SystemTime::UNIX_EPOCH,
        }))
    }

    async fn save_checkpoint(&self, _checkpoint: ProjectionCheckpoint) -> Result<(), PortError> {
        Ok(())
    }
}

struct SnapshotWriter;

impl SnapshotStore for SnapshotWriter {
    async fn save_bundle_with_options(
        &self,
        _bundle: &KmpBundle,
        _options: SnapshotSaveOptions,
    ) -> Result<(), PortError> {
        Ok(())
    }
}

#[tokio::test]
async fn projection_writer_delegates_through_arc() {
    let writer = Arc::new(Writer);
    writer
        .apply_mutations(vec![ProjectionMutation::UpsertNode(NodeProjection {
            node_id: "node-123".to_string(),
            node_kind: "capability".to_string(),
            title: "Projection".to_string(),
            summary: String::new(),
            status: "ACTIVE".to_string(),
            labels: vec!["projection".to_string()],
            properties: BTreeMap::new(),
            provenance: None,
        })])
        .await
        .expect("write should succeed");
}

#[tokio::test]
async fn processed_event_store_delegates_through_arc() {
    let store = Arc::new(EventStore);
    assert!(
        store
            .has_processed("context-projection", "evt-1")
            .await
            .expect("lookup should succeed")
    );
}

#[tokio::test]
async fn checkpoint_store_delegates_through_arc() {
    let store = Arc::new(CheckpointStore);
    let checkpoint = store
        .load_checkpoint("context-projection", "rehydration.events")
        .await
        .expect("load should succeed")
        .expect("checkpoint should exist");

    assert_eq!(checkpoint.last_event_id, "evt-1");
}

#[tokio::test]
async fn snapshot_store_delegates_through_arc() {
    let store = Arc::new(SnapshotWriter);
    let bundle = sample_bundle();

    store
        .save_bundle(&bundle)
        .await
        .expect("save should succeed");
}

#[test]
fn mutation_variants_stay_node_centric() {
    let mutation = ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
        node_id: "node-123".to_string(),
        detail: "expanded detail".to_string(),
        content_hash: "hash-1".to_string(),
        revision: 1,
    });

    match mutation {
        ProjectionMutation::UpsertNodeDetail(detail) => assert_eq!(detail.node_id, "node-123"),
        other => panic!("unexpected mutation: {other:?}"),
    }
}

fn sample_bundle() -> KmpBundle {
    let case_id = CaseId::new("case-123").expect("case id is valid");
    let role = Role::new("developer").expect("role is valid");

    KmpBundle::new(
        case_id.clone(),
        role,
        BundleNode::new(
            case_id.as_str(),
            "capability",
            "Node case-123",
            "bundle for node case-123 role developer",
            "ACTIVE",
            vec!["projection-node".to_string()],
            BTreeMap::new(),
        ),
        Vec::new(),
        Vec::new(),
        vec![BundleNodeDetail::new(
            case_id.as_str(),
            "bundle for node case-123 role developer",
            "pending",
            1,
        )],
        BundleMetadata::initial("0.1.0"),
    )
    .expect("bundle should be valid")
}
