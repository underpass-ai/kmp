//! Conformance arm (b): the same suite that runs against the in-memory
//! kernel store, executed against the containerized infrastructure adapters
//! (Neo4j graph store + Valkey detail/snapshot/event/runtime-state stores).
//!
//! Containers start once; each scenario gets an isolated backend by clearing
//! the graph and namespacing the Valkey key prefixes per scenario.

#![cfg(feature = "container-tests")]

use std::sync::atomic::{AtomicUsize, Ordering};

use rehydration_adapter_neo4j::Neo4jProjectionStore;
use rehydration_adapter_valkey::{
    ValkeyContextEventStore, ValkeyNodeDetailStore, ValkeyProcessedEventStore,
    ValkeyProjectionCheckpointStore, ValkeySnapshotStore,
};
use rehydration_conformance::{
    ConformanceBackend, ConformanceBackendFactory, FactoryBackend, scenarios,
};
use rehydration_tests_shared::containers::{Neo4jContainer, ValkeyContainer};

const VALKEY_TTL_SECONDS: u32 = 300;

struct AdapterBackendFactory {
    neo4j: Neo4jContainer,
    valkey: ValkeyContainer,
    scenario_counter: AtomicUsize,
}

impl AdapterBackendFactory {
    async fn start() -> Self {
        let neo4j = Neo4jContainer::start()
            .await
            .expect("neo4j container should start");
        let valkey = ValkeyContainer::start()
            .await
            .expect("valkey container should start");
        Self {
            neo4j,
            valkey,
            scenario_counter: AtomicUsize::new(0),
        }
    }

    fn valkey_uri(&self, scenario: usize, store: &str) -> String {
        self.valkey.endpoint().redis_uri(
            &format!("conformance:{scenario}:{store}"),
            VALKEY_TTL_SECONDS,
        )
    }
}

impl ConformanceBackendFactory for AdapterBackendFactory {
    type Graph = Neo4jProjectionStore;
    type Detail = ValkeyNodeDetailStore;
    type Snapshot = ValkeySnapshotStore;
    type Events = ValkeyContextEventStore;
    type Processed = ValkeyProcessedEventStore;
    type Checkpoints = ValkeyProjectionCheckpointStore;

    async fn fresh(&self) -> FactoryBackend<Self> {
        self.neo4j
            .clear()
            .await
            .expect("neo4j graph should clear between scenarios");
        let scenario = self.scenario_counter.fetch_add(1, Ordering::SeqCst);

        ConformanceBackend::new(
            self.neo4j.graph_store().expect("neo4j graph store"),
            ValkeyNodeDetailStore::new(self.valkey_uri(scenario, "detail"))
                .expect("valkey detail store"),
            ValkeySnapshotStore::new(self.valkey_uri(scenario, "snapshot"))
                .expect("valkey snapshot store"),
            ValkeyContextEventStore::new(self.valkey_uri(scenario, "events"))
                .expect("valkey context event store"),
            ValkeyProcessedEventStore::new(self.valkey_uri(scenario, "processed"))
                .expect("valkey processed event store"),
            ValkeyProjectionCheckpointStore::new(self.valkey_uri(scenario, "checkpoints"))
                .expect("valkey checkpoint store"),
        )
    }
}

macro_rules! run_scenario {
    ($factory:expr, $scenario:ident) => {
        eprintln!("conformance scenario: {}", stringify!($scenario));
        scenarios::$scenario(&$factory).await;
    };
}

#[tokio::test]
async fn conformance_suite_passes_against_neo4j_and_valkey_adapters() {
    let factory = AdapterBackendFactory::start().await;

    run_scenario!(factory, write_read_coherence_projected_nodes_are_readable);
    run_scenario!(factory, ensure_node_preserves_existing_upsert_overwrites);
    run_scenario!(
        factory,
        relation_upsert_creates_placeholder_endpoints_and_is_idempotent
    );
    run_scenario!(
        factory,
        neighborhood_traversal_is_depth_bounded_and_directed
    );
    run_scenario!(
        factory,
        context_path_resolves_shortest_path_with_target_subtree
    );
    run_scenario!(factory, node_relationships_split_incoming_and_outgoing);
    run_scenario!(factory, about_index_lists_anchors_and_filters_by_dimension);
    run_scenario!(factory, node_detail_upsert_is_last_write_wins);
    run_scenario!(factory, projection_events_dedup_by_event_id);
    run_scenario!(factory, projection_event_replay_converges_to_same_state);
    run_scenario!(factory, ingest_then_wake_is_read_after_write_consistent);
    run_scenario!(factory, ingest_dry_run_writes_nothing);
    run_scenario!(
        factory,
        ingest_idempotency_replay_is_safe_and_conflicts_fail
    );
    run_scenario!(factory, temporal_moves_navigate_known_at_time_coordinates);
    run_scenario!(factory, inspect_surfaces_relation_proof);
    run_scenario!(factory, trace_resolves_path_between_anchor_and_entry);
}
