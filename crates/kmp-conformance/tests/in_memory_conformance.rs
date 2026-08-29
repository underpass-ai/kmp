//! Conformance arm (a): the suite against the coherent in-memory kernel
//! store. This is the fastest arm and the reference embedded adapters are
//! developed against.

use kmp_conformance::{ConformanceBackend, ConformanceBackendFactory, scenarios};
use kmp_testkit::{
    InMemoryContextEventStore, InMemoryKernelStore, InMemoryProcessedEventStore,
    InMemoryProjectionCheckpointStore, NoopSnapshotStore,
};

struct InMemoryFactory;

impl ConformanceBackendFactory for InMemoryFactory {
    type Graph = InMemoryKernelStore;
    type Detail = InMemoryKernelStore;
    type Snapshot = NoopSnapshotStore;
    type Events = InMemoryContextEventStore;
    type Processed = InMemoryProcessedEventStore;
    type Checkpoints = InMemoryProjectionCheckpointStore;

    async fn fresh(
        &self,
    ) -> ConformanceBackend<
        InMemoryKernelStore,
        InMemoryKernelStore,
        NoopSnapshotStore,
        InMemoryContextEventStore,
        InMemoryProcessedEventStore,
        InMemoryProjectionCheckpointStore,
    > {
        let store = InMemoryKernelStore::default();
        ConformanceBackend::new(
            store.clone(),
            store,
            NoopSnapshotStore,
            InMemoryContextEventStore::new(),
            InMemoryProcessedEventStore::default(),
            InMemoryProjectionCheckpointStore::default(),
        )
    }
}

#[tokio::test]
async fn write_read_coherence_projected_nodes_are_readable() {
    scenarios::write_read_coherence_projected_nodes_are_readable(&InMemoryFactory).await;
}

#[tokio::test]
async fn ensure_node_preserves_existing_upsert_overwrites() {
    scenarios::ensure_node_preserves_existing_upsert_overwrites(&InMemoryFactory).await;
}

#[tokio::test]
async fn relation_upsert_creates_placeholder_endpoints_and_is_idempotent() {
    scenarios::relation_upsert_creates_placeholder_endpoints_and_is_idempotent(&InMemoryFactory)
        .await;
}

#[tokio::test]
async fn neighborhood_traversal_is_depth_bounded_and_directed() {
    scenarios::neighborhood_traversal_is_depth_bounded_and_directed(&InMemoryFactory).await;
}

#[tokio::test]
async fn context_path_resolves_shortest_path_with_target_subtree() {
    scenarios::context_path_resolves_shortest_path_with_target_subtree(&InMemoryFactory).await;
}

#[tokio::test]
async fn node_relationships_split_incoming_and_outgoing() {
    scenarios::node_relationships_split_incoming_and_outgoing(&InMemoryFactory).await;
}

#[tokio::test]
async fn about_index_lists_anchors_and_filters_by_dimension() {
    scenarios::about_index_lists_anchors_and_filters_by_dimension(&InMemoryFactory).await;
}

#[tokio::test]
async fn node_detail_upsert_is_last_write_wins() {
    scenarios::node_detail_upsert_is_last_write_wins(&InMemoryFactory).await;
}

#[tokio::test]
async fn projection_events_dedup_by_event_id() {
    scenarios::projection_events_dedup_by_event_id(&InMemoryFactory).await;
}

#[tokio::test]
async fn projection_event_replay_converges_to_same_state() {
    scenarios::projection_event_replay_converges_to_same_state(&InMemoryFactory).await;
}

#[tokio::test]
async fn ingest_then_wake_is_read_after_write_consistent() {
    scenarios::ingest_then_wake_is_read_after_write_consistent(&InMemoryFactory).await;
}

#[tokio::test]
async fn ingest_dry_run_writes_nothing() {
    scenarios::ingest_dry_run_writes_nothing(&InMemoryFactory).await;
}

#[tokio::test]
async fn ingest_idempotency_replay_is_safe_and_conflicts_fail() {
    scenarios::ingest_idempotency_replay_is_safe_and_conflicts_fail(&InMemoryFactory).await;
}

#[tokio::test]
async fn temporal_moves_navigate_known_at_time_coordinates() {
    scenarios::temporal_moves_navigate_known_at_time_coordinates(&InMemoryFactory).await;
}

#[tokio::test]
async fn inspect_surfaces_relation_proof() {
    scenarios::inspect_surfaces_relation_proof(&InMemoryFactory).await;
}

#[tokio::test]
async fn trace_resolves_path_between_anchor_and_entry() {
    scenarios::trace_resolves_path_between_anchor_and_entry(&InMemoryFactory).await;
}
