//! Conformance arm (d): the suite against the embedded adapters on the
//! SQLite engine (ADR-018). Same sixteen scenarios as the redb arm, and the
//! acceptance criterion for the engine: an engine that does not pass every
//! one of them is not a valid backend, whatever else it can do.
//!
//! Every scenario opens a fresh data directory stamped for SQLite, so this
//! arm also exercises the engine's table creation and the format gate's
//! second layout.

#![cfg(feature = "sqlite")]

use std::sync::Mutex;

use kmp_adapter_embedded::{EmbeddedKernelStore, StorageEngine};
use kmp_conformance::{ConformanceBackend, ConformanceBackendFactory, FactoryBackend, scenarios};

struct EmbeddedSqliteFactory {
    data_dirs: Mutex<Vec<tempfile::TempDir>>,
}

impl EmbeddedSqliteFactory {
    fn new() -> Self {
        Self {
            data_dirs: Mutex::new(Vec::new()),
        }
    }
}

impl ConformanceBackendFactory for EmbeddedSqliteFactory {
    type Graph = EmbeddedKernelStore;
    type Detail = EmbeddedKernelStore;
    type Snapshot = EmbeddedKernelStore;
    type Events = EmbeddedKernelStore;
    type Processed = EmbeddedKernelStore;
    type Checkpoints = EmbeddedKernelStore;

    async fn fresh(&self) -> FactoryBackend<Self> {
        let data_dir = tempfile::tempdir().expect("temp data dir");
        let store = EmbeddedKernelStore::open_with_engine(data_dir.path(), StorageEngine::Sqlite)
            .expect("embedded sqlite store opens");
        self.data_dirs
            .lock()
            .expect("data dir registry")
            .push(data_dir);
        ConformanceBackend::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store,
        )
    }
}

macro_rules! embedded_sqlite_scenario {
    ($name:ident) => {
        #[tokio::test]
        async fn $name() {
            scenarios::$name(&EmbeddedSqliteFactory::new()).await;
        }
    };
}

embedded_sqlite_scenario!(write_read_coherence_projected_nodes_are_readable);
embedded_sqlite_scenario!(ensure_node_preserves_existing_upsert_overwrites);
embedded_sqlite_scenario!(relation_upsert_creates_placeholder_endpoints_and_is_idempotent);
embedded_sqlite_scenario!(neighborhood_traversal_is_depth_bounded_and_directed);
embedded_sqlite_scenario!(context_path_resolves_shortest_path_with_target_subtree);
embedded_sqlite_scenario!(node_relationships_split_incoming_and_outgoing);
embedded_sqlite_scenario!(about_index_lists_anchors_and_filters_by_dimension);
embedded_sqlite_scenario!(node_detail_upsert_is_last_write_wins);
embedded_sqlite_scenario!(projection_events_dedup_by_event_id);
embedded_sqlite_scenario!(projection_event_replay_converges_to_same_state);
embedded_sqlite_scenario!(ingest_then_wake_is_read_after_write_consistent);
embedded_sqlite_scenario!(ingest_dry_run_writes_nothing);
embedded_sqlite_scenario!(ingest_idempotency_replay_is_safe_and_conflicts_fail);
embedded_sqlite_scenario!(temporal_moves_navigate_known_at_time_coordinates);
embedded_sqlite_scenario!(inspect_surfaces_relation_proof);
embedded_sqlite_scenario!(trace_resolves_path_between_anchor_and_entry);
