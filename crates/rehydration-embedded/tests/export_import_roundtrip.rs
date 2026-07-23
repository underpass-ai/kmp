//! E6 acceptance: export a store, import into a fresh one, and verify that
//! wake content, known-at-time temporal reads and relation proof are
//! identical across the round trip.

use rehydration_application::{
    InspectMemoryQuery, MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData,
    MemoryEvidenceData, MemoryIngestCommand, TemporalIncludeOptions, TemporalMemoryQuery,
    WakeMemoryQuery, projection_mutations_for_context_event,
};
use rehydration_domain::{DimensionSelection, TemporalCursor, TemporalDirection, TemporalWindow};
use rehydration_embedded::EmbeddedKernel;

const ABOUT: &str = "project:roundtrip";

fn entry(id: &str, text: &str, occurred_at: &str, sequence: u32) -> MemoryEntryData {
    MemoryEntryData {
        id: id.to_string(),
        kind: "decision".to_string(),
        text: text.to_string(),
        coordinates: vec![MemoryCoordinateData {
            dimension: "timeline".to_string(),
            scope_id: "timeline:work".to_string(),
            occurred_at: Some(occurred_at.to_string()),
            observed_at: None,
            ingested_at: None,
            valid_from: None,
            valid_until: None,
            sequence: Some(sequence),
            rank: None,
            metadata: Default::default(),
        }],
        metadata: Default::default(),
    }
}

fn corpus(idempotency_key: &str) -> MemoryIngestCommand {
    MemoryIngestCommand {
        about: ABOUT.to_string(),
        memory: MemoryData {
            dimensions: vec![MemoryDimensionData {
                id: "timeline:work".to_string(),
                kind: "timeline".to_string(),
                title: None,
                metadata: Default::default(),
            }],
            entries: vec![
                entry(
                    "decision:first",
                    "First decision.",
                    "2026-07-01T10:00:00Z",
                    1,
                ),
                entry(
                    "decision:second",
                    "Second decision.",
                    "2026-07-02T10:00:00Z",
                    2,
                ),
            ],
            relations: vec![],
            evidence: vec![MemoryEvidenceData {
                id: "evidence:first".to_string(),
                supports: vec!["decision:first".to_string()],
                text: "Proof for the first decision.".to_string(),
                source: None,
                time: None,
                metadata: Default::default(),
            }],
        },
        provenance: None,
        idempotency_key: idempotency_key.to_string(),
        dry_run: false,
    }
}

async fn wake_node_ids(kernel: &EmbeddedKernel) -> Vec<String> {
    let result = kernel
        .service()
        .wake(WakeMemoryQuery {
            about: ABOUT.to_string(),
            role: "resumer".to_string(),
            intent: "roundtrip".to_string(),
            dimensions: DimensionSelection::all(),
            token_budget: 4096,
            depth: 2,
            max_tier: None,
            max_entries: None,
        })
        .await
        .expect("wake succeeds");
    let mut ids: Vec<String> = result
        .bundle
        .neighbor_nodes()
        .iter()
        .map(|node| node.node_id().to_string())
        .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn export_import_preserves_wake_temporal_and_proof() {
    let source_dir = tempfile::tempdir().expect("source dir");
    let source = EmbeddedKernel::open(source_dir.path()).expect("source opens");
    source
        .service()
        .ingest(corpus("ingest:roundtrip-1"))
        .await
        .expect("ingest succeeds");

    let bundle = source.store().export_bundle().await.expect("export");

    let target_dir = tempfile::tempdir().expect("target dir");
    let target = EmbeddedKernel::open(target_dir.path()).expect("target opens");
    let report = target
        .store()
        .import_bundle(&bundle, projection_mutations_for_context_event)
        .await
        .expect("import succeeds");
    assert_eq!(report.events_imported, 1);

    // Wake parity.
    assert_eq!(wake_node_ids(&source).await, wake_node_ids(&target).await);

    // Known-at-time parity: goto resolves the same entry on both stores.
    for kernel in [&source, &target] {
        let goto = kernel
            .service()
            .temporal(TemporalMemoryQuery {
                about: ABOUT.to_string(),
                direction: TemporalDirection::Goto,
                cursor: TemporalCursor::time("2026-07-01T12:00:00Z").expect("cursor"),
                dimensions: DimensionSelection::all(),
                window: TemporalWindow::new(0, 0),
                limit_entries: None,
                include: TemporalIncludeOptions::default(),
                token_budget: 4096,
                depth: 2,
                max_tier: None,
            })
            .await
            .expect("goto succeeds");
        assert_eq!(goto.traversal.entries()[0].ref_id(), "decision:first");
    }

    // Proof parity: the evidence supports relation survives with rationale.
    let inspection = target
        .service()
        .inspect(InspectMemoryQuery {
            ref_id: "decision:first".to_string(),
            include_details: true,
            include_incoming: true,
            include_outgoing: false,
            include_raw: false,
        })
        .await
        .expect("inspect succeeds");
    let supports = inspection
        .incoming
        .iter()
        .find(|relationship| {
            relationship.relationship_type == "supports"
                && relationship.source_node_id == "evidence:first"
        })
        .expect("imported store keeps the proof relation");
    assert!(supports.explanation.evidence().is_some());

    // Fail-fast: importing into a non-empty store is rejected.
    let error = target
        .store()
        .import_bundle(&bundle, projection_mutations_for_context_event)
        .await
        .expect_err("second import must fail");
    assert!(error.to_string().contains("empty store"));
}
