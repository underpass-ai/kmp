//! Compatibility conformance arm: the suite against a format-1 redb store.
//! New stores use SQLite; this arm pins the promise that older memory remains
//! readable and behaviorally intact during its migration window.

use std::sync::Mutex;

use kmp_adapter_embedded::{EmbeddedKernelStore, StorageEngine};
use kmp_conformance::{ConformanceBackend, ConformanceBackendFactory, FactoryBackend, scenarios};

struct EmbeddedFactory {
    data_dirs: Mutex<Vec<tempfile::TempDir>>,
}

impl EmbeddedFactory {
    fn new() -> Self {
        Self {
            data_dirs: Mutex::new(Vec::new()),
        }
    }
}

impl ConformanceBackendFactory for EmbeddedFactory {
    type Graph = EmbeddedKernelStore;
    type Detail = EmbeddedKernelStore;
    type Snapshot = EmbeddedKernelStore;
    type Events = EmbeddedKernelStore;
    type Processed = EmbeddedKernelStore;
    type Checkpoints = EmbeddedKernelStore;

    async fn fresh(&self) -> FactoryBackend<Self> {
        let data_dir = tempfile::tempdir().expect("temp data dir");
        std::fs::write(data_dir.path().join("FORMAT_VERSION"), "1\n")
            .expect("legacy format stamp");
        let store = EmbeddedKernelStore::open_with_engine(data_dir.path(), StorageEngine::Redb)
            .expect("legacy embedded store opens");
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

macro_rules! embedded_scenario {
    ($name:ident) => {
        #[tokio::test]
        async fn $name() {
            scenarios::$name(&EmbeddedFactory::new()).await;
        }
    };
}

embedded_scenario!(write_read_coherence_projected_nodes_are_readable);
embedded_scenario!(ensure_node_preserves_existing_upsert_overwrites);
embedded_scenario!(relation_upsert_creates_placeholder_endpoints_and_is_idempotent);
embedded_scenario!(neighborhood_traversal_is_depth_bounded_and_directed);
embedded_scenario!(context_path_resolves_shortest_path_with_target_subtree);
embedded_scenario!(node_relationships_split_incoming_and_outgoing);
embedded_scenario!(about_index_lists_anchors_and_filters_by_dimension);
embedded_scenario!(node_detail_upsert_is_last_write_wins);
embedded_scenario!(projection_events_dedup_by_event_id);
embedded_scenario!(projection_event_replay_converges_to_same_state);
embedded_scenario!(ingest_then_wake_is_read_after_write_consistent);
embedded_scenario!(ingest_dry_run_writes_nothing);
embedded_scenario!(ingest_idempotency_replay_is_safe_and_conflicts_fail);
embedded_scenario!(temporal_moves_navigate_known_at_time_coordinates);
embedded_scenario!(inspect_surfaces_relation_proof);
embedded_scenario!(trace_resolves_path_between_anchor_and_entry);
