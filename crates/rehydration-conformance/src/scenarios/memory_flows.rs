//! KMP service-level flows: ingest through the memory facade, then observe
//! the memory through wake/temporal/inspect/trace. These scenarios pin
//! read-after-write consistency, ingest idempotency, known-at-time temporal
//! navigation, and relation proof — the product guarantees both editions
//! must share.

use std::collections::BTreeSet;

use rehydration_application::{
    ApplicationError, InspectMemoryQuery, MemoryCoordinateData, MemoryData, MemoryDimensionData,
    MemoryEntryData, MemoryEvidenceData, MemoryIngestCommand, MemoryRelationData,
    TemporalIncludeOptions, TemporalMemoryQuery, TraceMemoryQuery, TracePageRequest,
    WakeMemoryQuery,
};
use rehydration_domain::{
    DimensionSelection, RehydrationBundle, RelationSemanticClass, TemporalCursor,
    TemporalDirection, TemporalWindow,
};

use crate::ConformanceBackendFactory;

const ABOUT: &str = "question:conformance";
const SCOPE_ALIAS: &str = "conversation:session-a";

fn namespaced_scope() -> String {
    format!("about:{ABOUT}:dimension:{SCOPE_ALIAS}")
}

fn coordinate(occurred_at: &str, sequence: u32) -> MemoryCoordinateData {
    MemoryCoordinateData {
        dimension: "conversation".to_string(),
        scope_id: SCOPE_ALIAS.to_string(),
        occurred_at: Some(occurred_at.to_string()),
        observed_at: None,
        ingested_at: None,
        valid_from: None,
        valid_until: None,
        sequence: Some(sequence),
        rank: None,
        metadata: Default::default(),
    }
}

fn entry(id: &str, text: &str, occurred_at: &str, sequence: u32) -> MemoryEntryData {
    MemoryEntryData {
        id: id.to_string(),
        kind: "claim".to_string(),
        text: text.to_string(),
        coordinates: vec![coordinate(occurred_at, sequence)],
        metadata: Default::default(),
    }
}

fn conversation_memory_command(idempotency_key: &str) -> MemoryIngestCommand {
    MemoryIngestCommand {
        about: ABOUT.to_string(),
        memory: MemoryData {
            dimensions: vec![MemoryDimensionData {
                id: SCOPE_ALIAS.to_string(),
                kind: "conversation".to_string(),
                title: Some("Conformance conversation".to_string()),
                metadata: Default::default(),
            }],
            entries: vec![
                entry("claim:one", "First decision.", "2026-07-01T10:00:00Z", 1),
                entry("claim:two", "Second decision.", "2026-07-02T10:00:00Z", 2),
                entry("claim:three", "Third decision.", "2026-07-03T10:00:00Z", 3),
            ],
            relations: vec![MemoryRelationData {
                source_ref: "claim:two".to_string(),
                target_ref: "claim:one".to_string(),
                rel: "supports".to_string(),
                semantic_class: "evidential".to_string(),
                why: Some("Second decision reinforces the first.".to_string()),
                evidence: None,
                confidence: Some("high".to_string()),
                sequence: None,
                motivation: None,
                method: None,
                decision_id: None,
                caused_by_node_id: None,
                coordinate: None,
            }],
            evidence: vec![MemoryEvidenceData {
                id: "evidence:one".to_string(),
                supports: vec!["claim:one".to_string()],
                text: "Transcript line supporting the first decision.".to_string(),
                source: Some("transcript:1".to_string()),
                time: Some("2026-07-01T10:00:00Z".to_string()),
                metadata: Default::default(),
            }],
        },
        provenance: None,
        idempotency_key: idempotency_key.to_string(),
        dry_run: false,
    }
}

fn wake_query() -> WakeMemoryQuery {
    WakeMemoryQuery {
        about: ABOUT.to_string(),
        role: "resumer".to_string(),
        intent: "resume conformance work".to_string(),
        dimensions: DimensionSelection::all(),
        token_budget: 4096,
        depth: 2,
        max_tier: None,
        max_entries: None,
    }
}

fn temporal_query(
    direction: TemporalDirection,
    cursor: TemporalCursor,
    window: TemporalWindow,
) -> TemporalMemoryQuery {
    TemporalMemoryQuery {
        about: ABOUT.to_string(),
        direction,
        cursor,
        dimensions: DimensionSelection::all(),
        window,
        limit_entries: None,
        include: TemporalIncludeOptions::default(),
        token_budget: 4096,
        depth: 2,
        max_tier: None,
    }
}

fn bundle_node_ids(bundle: &RehydrationBundle) -> BTreeSet<String> {
    bundle
        .neighbor_nodes()
        .iter()
        .map(|node| node.node_id().to_string())
        .collect()
}

pub async fn ingest_then_wake_is_read_after_write_consistent(
    factory: &impl ConformanceBackendFactory,
) {
    let backend = factory.fresh().await;
    let memory = backend.memory_service();

    let outcome = memory
        .ingest(conversation_memory_command("ingest:conformance-wake"))
        .await
        .expect("ingest should succeed");
    assert_eq!(outcome.about, ABOUT);
    assert_eq!(outcome.accepted.entries, 3);
    assert!(
        outcome.read_after_write_ready,
        "synchronous projection must report read_after_write_ready"
    );

    let wake = memory
        .wake(wake_query())
        .await
        .expect("wake immediately after ingest must succeed");
    assert_eq!(wake.bundle.root_node().node_id(), ABOUT);
    let node_ids = bundle_node_ids(&wake.bundle);
    for expected in [
        namespaced_scope(),
        "claim:one".to_string(),
        "claim:two".to_string(),
        "claim:three".to_string(),
        "evidence:one".to_string(),
    ] {
        assert!(
            node_ids.contains(&expected),
            "wake bundle must surface `{expected}` right after ingest; got {node_ids:?}"
        );
    }

    let abouts = memory
        .wake(wake_query())
        .await
        .expect("repeated wake must stay consistent");
    assert_eq!(bundle_node_ids(&abouts.bundle), node_ids);
}

pub async fn ingest_dry_run_writes_nothing(factory: &impl ConformanceBackendFactory) {
    let backend = factory.fresh().await;
    let memory = backend.memory_service();

    let mut command = conversation_memory_command("ingest:conformance-dry");
    command.dry_run = true;
    let outcome = memory
        .ingest(command)
        .await
        .expect("dry-run ingest should validate");
    assert!(
        !outcome.read_after_write_ready,
        "dry-run must not report read-after-write readiness"
    );
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning.contains("dry_run")),
        "dry-run must warn that nothing was written"
    );

    let wake = memory.wake(wake_query()).await;
    assert!(
        matches!(wake, Err(ApplicationError::NotFound(_))),
        "dry-run must leave no readable memory behind"
    );
}

pub async fn ingest_idempotency_replay_is_safe_and_conflicts_fail(
    factory: &impl ConformanceBackendFactory,
) {
    let backend = factory.fresh().await;
    let memory = backend.memory_service();

    let first = memory
        .ingest(conversation_memory_command("ingest:conformance-idem"))
        .await
        .expect("first ingest should succeed");
    let baseline = bundle_node_ids(
        &memory
            .wake(wake_query())
            .await
            .expect("wake should succeed")
            .bundle,
    );

    // Retrying the same logical ingest is not byte-identical after
    // translation (the refs now exist), so the kernel compares the logical
    // digest taken *before* translation. The conformance property is that an
    // at-least-once pipeline can apply twice and store once: the replay
    // answers with the recorded outcome and leaves memory unchanged.
    let replay = memory
        .ingest(conversation_memory_command("ingest:conformance-idem"))
        .await
        .expect("same-key same-content retry must replay the recorded outcome");
    assert_eq!(
        replay.memory_id, first.memory_id,
        "a replay answers for the same memory, not a second one"
    );
    let after_replay = bundle_node_ids(
        &memory
            .wake(wake_query())
            .await
            .expect("wake should succeed")
            .bundle,
    );
    assert_eq!(
        after_replay, baseline,
        "a replayed ingest must not change readable memory"
    );

    let mut conflicting = conversation_memory_command("ingest:conformance-idem");
    conflicting.memory.entries[0].text = "Rewritten first decision.".to_string();
    let conflict = memory.ingest(conflicting).await;
    assert!(
        conflict.is_err(),
        "same idempotency key with different content must be rejected"
    );
}

pub async fn temporal_moves_navigate_known_at_time_coordinates(
    factory: &impl ConformanceBackendFactory,
) {
    let backend = factory.fresh().await;
    let memory = backend.memory_service();
    memory
        .ingest(conversation_memory_command("ingest:conformance-temporal"))
        .await
        .expect("ingest should succeed");

    let goto = memory
        .temporal(temporal_query(
            TemporalDirection::Goto,
            TemporalCursor::time("2026-07-02T12:00:00Z").expect("valid cursor"),
            TemporalWindow::new(0, 0),
        ))
        .await
        .expect("goto should succeed");
    let goto_refs = goto
        .traversal
        .entries()
        .iter()
        .map(|entry| entry.ref_id().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        goto_refs,
        vec!["claim:one".to_string(), "claim:two".to_string()],
        "goto by time must return the entries known at the cursor within its default page"
    );

    let forward = memory
        .temporal(temporal_query(
            TemporalDirection::Forward,
            TemporalCursor::ref_id("claim:one").expect("valid cursor"),
            TemporalWindow::new(0, 1),
        ))
        .await
        .expect("forward should succeed");
    let forward_refs = forward
        .traversal
        .entries()
        .iter()
        .map(|entry| entry.ref_id().to_string())
        .collect::<BTreeSet<_>>();
    assert!(
        forward_refs.contains("claim:two"),
        "forward from claim:one must reach claim:two; got {forward_refs:?}"
    );
    assert!(
        !forward_refs.contains("claim:one"),
        "forward must move off the cursor entry"
    );

    let rewind = memory
        .temporal(temporal_query(
            TemporalDirection::Rewind,
            TemporalCursor::ref_id("claim:three").expect("valid cursor"),
            TemporalWindow::new(1, 0),
        ))
        .await
        .expect("rewind should succeed");
    let rewind_refs = rewind
        .traversal
        .entries()
        .iter()
        .map(|entry| entry.ref_id().to_string())
        .collect::<BTreeSet<_>>();
    assert!(
        rewind_refs.contains("claim:two"),
        "rewind from claim:three must reach claim:two; got {rewind_refs:?}"
    );
    assert!(
        !rewind_refs.contains("claim:three"),
        "rewind must move off the cursor entry"
    );

    let near = memory
        .temporal(temporal_query(
            TemporalDirection::Near,
            TemporalCursor::ref_id("claim:two").expect("valid cursor"),
            TemporalWindow::new(1, 1),
        ))
        .await
        .expect("near should succeed");
    let near_refs = near
        .traversal
        .entries()
        .iter()
        .map(|entry| entry.ref_id().to_string())
        .collect::<BTreeSet<_>>();
    for expected in ["claim:one", "claim:three"] {
        assert!(
            near_refs.contains(expected),
            "near claim:two with window 1/1 must include `{expected}`; got {near_refs:?}"
        );
    }
}

pub async fn inspect_surfaces_relation_proof(factory: &impl ConformanceBackendFactory) {
    let backend = factory.fresh().await;
    let memory = backend.memory_service();
    memory
        .ingest(conversation_memory_command("ingest:conformance-inspect"))
        .await
        .expect("ingest should succeed");

    let inspection = memory
        .inspect(InspectMemoryQuery {
            ref_id: "claim:one".to_string(),
            include_details: true,
            include_incoming: true,
            include_outgoing: true,
            include_raw: true,
        })
        .await
        .expect("inspect should succeed");

    assert!(
        inspection.detail.detail.is_some(),
        "inspect must surface the stored entry detail"
    );

    let contains_entry = inspection
        .incoming
        .iter()
        .find(|relationship| relationship.relationship_type == "contains_entry")
        .expect("inspect must surface the containing scope relation");
    assert_eq!(contains_entry.source_node_id, namespaced_scope());
    assert_eq!(
        contains_entry.explanation.dimension(),
        Some("conversation"),
        "contains_entry proof must carry the coordinate dimension"
    );
    assert_eq!(
        contains_entry.explanation.occurred_at(),
        Some("2026-07-01T10:00:00Z"),
        "contains_entry proof must carry the coordinate timestamp"
    );

    let supports = inspection
        .incoming
        .iter()
        .find(|relationship| {
            relationship.relationship_type == "supports"
                && relationship.source_node_id == "evidence:one"
        })
        .expect("inspect must surface the evidence supports relation");
    assert_eq!(
        supports.explanation.semantic_class(),
        &RelationSemanticClass::Evidential
    );
    assert!(
        supports.explanation.rationale().is_some(),
        "supports proof must carry a rationale"
    );
    assert!(
        supports.explanation.evidence().is_some(),
        "supports proof must carry the evidence text"
    );

    let claim_support = inspection
        .incoming
        .iter()
        .find(|relationship| {
            relationship.relationship_type == "supports"
                && relationship.source_node_id == "claim:two"
        })
        .expect("inspect must surface writer-declared relations");
    assert_eq!(
        claim_support.explanation.confidence(),
        Some("high"),
        "writer-declared proof fields must survive storage"
    );

    assert!(
        !inspection.raw_coordinates.is_empty(),
        "inspect with include_raw must expose temporal coordinates"
    );
}

pub async fn trace_resolves_path_between_anchor_and_entry(
    factory: &impl ConformanceBackendFactory,
) {
    let backend = factory.fresh().await;
    let memory = backend.memory_service();
    memory
        .ingest(conversation_memory_command("ingest:conformance-trace"))
        .await
        .expect("ingest should succeed");

    let trace = memory
        .trace(TraceMemoryQuery {
            from: ABOUT.to_string(),
            to: "claim:two".to_string(),
            role: "memory".to_string(),
            token_budget: 4096,
            page: TracePageRequest::default(),
        })
        .await
        .expect("trace between anchor and entry must resolve");
    assert_eq!(trace.path_bundle.root_node().node_id(), ABOUT);
    assert!(
        bundle_node_ids(&trace.path_bundle).contains("claim:two"),
        "trace bundle must include the target entry"
    );

    let unreachable = memory
        .trace(TraceMemoryQuery {
            from: ABOUT.to_string(),
            to: "claim:absent".to_string(),
            role: "memory".to_string(),
            token_budget: 4096,
            page: TracePageRequest::default(),
        })
        .await;
    assert!(
        unreachable.is_err(),
        "trace to an unknown ref must fail explicitly, never fabricate a path"
    );
}
