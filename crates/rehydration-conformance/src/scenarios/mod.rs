//! Conformance scenarios. Each function takes a factory, builds an isolated
//! backend, and panics with a descriptive message on any semantic violation.

mod memory_flows;
mod projection_events;
mod projection_storage;

pub use memory_flows::{
    ingest_dry_run_writes_nothing, ingest_idempotency_replay_is_safe_and_conflicts_fail,
    ingest_then_wake_is_read_after_write_consistent, inspect_surfaces_relation_proof,
    temporal_moves_navigate_known_at_time_coordinates,
    trace_resolves_path_between_anchor_and_entry,
};
pub use projection_events::{
    projection_event_replay_converges_to_same_state, projection_events_dedup_by_event_id,
};
pub use projection_storage::{
    about_index_lists_anchors_and_filters_by_dimension,
    context_path_resolves_shortest_path_with_target_subtree,
    ensure_node_preserves_existing_upsert_overwrites,
    neighborhood_traversal_is_depth_bounded_and_directed, node_detail_upsert_is_last_write_wins,
    node_relationships_split_incoming_and_outgoing,
    relation_upsert_creates_placeholder_endpoints_and_is_idempotent,
    write_read_coherence_projected_nodes_are_readable,
};
