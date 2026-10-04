//! `import --from --about` through the real kernel (#903): memory written by
//! the application in one store, read read-only while that store is open,
//! and incorporated into another store that already holds memory of its own.
//! The projections it leaves must be the ones a full rebuild would produce.

use kmp_application::{
    MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData, MemoryEvidenceData,
    MemoryIngestCommand, WakeMemoryQuery, projection_mutations_for_context_event,
};
use kmp_domain::DimensionSelection;
use kmp_embedded::{AboutImportOutcome, EmbeddedKernel, EmbeddedKernelStore};

fn write(about: &str, key: &str, slug: &str, text: &str) -> MemoryIngestCommand {
    let timeline = format!("timeline:{about}");
    let entry_id = format!("{about}:decision:{slug}");
    MemoryIngestCommand {
        receipt_context: None,
        default_observation_to_ingestion: false,
        neighborhood_review: None,
        about: about.to_string(),
        memory: MemoryData {
            dimensions: vec![MemoryDimensionData {
                id: timeline.clone(),
                kind: "timeline".to_string(),
                title: None,
                metadata: Default::default(),
            }],
            entries: vec![MemoryEntryData {
                id: entry_id.clone(),
                kind: "decision".to_string(),
                text: text.to_string(),
                coordinates: vec![MemoryCoordinateData {
                    dimension: "timeline".to_string(),
                    scope_id: timeline,
                    occurred_at: Some("2026-10-01T10:00:00Z".to_string()),
                    observed_at: None,
                    ingested_at: None,
                    valid_from: None,
                    valid_until: None,
                    sequence: None,
                    rank: None,
                    metadata: Default::default(),
                }],
                metadata: Default::default(),
            }],
            relations: vec![],
            evidence: vec![MemoryEvidenceData {
                support_clocks: None,
                id: format!("evidence:{about}:{slug}"),
                supports: vec![entry_id],
                text: format!("Proof for {slug}."),
                source: None,
                time: None,
                metadata: Default::default(),
            }],
        },
        provenance: None,
        idempotency_key: key.to_string(),
        dry_run: false,
        label_policy: Default::default(),
    }
}

async fn wake(kernel: &EmbeddedKernel, about: &str) -> Vec<String> {
    let result = kernel
        .service()
        .wake(WakeMemoryQuery {
            about: about.to_string(),
            role: "resumer".to_string(),
            intent: "import".to_string(),
            dimensions: DimensionSelection::all(),
            token_budget: 8192,
            depth: 2,
            max_tier: None,
            max_entries: None,
            temporal: kmp_domain::TemporalSelection::Frontier,
        })
        .await
        .expect("wake succeeds");
    let mut seen = result
        .bundle
        .neighbor_nodes()
        .iter()
        .map(|node| node.node_id().to_string())
        .chain(result.bundle.relationships().iter().map(|relation| {
            format!(
                "{} -{}-> {}",
                relation.source_node_id(),
                relation.relationship_type(),
                relation.target_node_id()
            )
        }))
        .collect::<Vec<_>>();
    seen.sort();
    seen
}

async fn read_only_bundle(dir: &std::path::Path, abouts: &[String]) -> String {
    EmbeddedKernelStore::open_read_only(dir)
        .expect("source opens read-only")
        .export_bundle_for_abouts(abouts)
        .await
        .expect("filtered export")
}

#[tokio::test]
async fn imported_memory_reads_like_the_source_and_survives_a_rebuild() {
    const ABOUT: &str = "project:shared";
    let requested = vec![ABOUT.to_string()];
    let source_dir = tempfile::tempdir().expect("source dir");
    let source = EmbeddedKernel::open(source_dir.path()).expect("source opens");
    for command in [
        write(ABOUT, "shared:1", "first", "Keep the store local."),
        write("project:private", "private:1", "secret", "Not for export."),
    ] {
        source.service().ingest(command).await.expect("ingest");
    }

    let target_dir = tempfile::tempdir().expect("target dir");
    let target = EmbeddedKernel::open(target_dir.path()).expect("target opens");
    target
        .service()
        .ingest(write("project:here", "here:1", "local", "Already here."))
        .await
        .expect("target memory");
    let here_before = wake(&target, "project:here").await;

    // The source stays open as a live writer while it is read.
    let report = target
        .store()
        .import_abouts(
            &read_only_bundle(source_dir.path(), &requested).await,
            &requested,
            projection_mutations_for_context_event,
        )
        .await
        .expect("import");
    assert_eq!(report.abouts[ABOUT], AboutImportOutcome::Imported);
    assert_eq!(wake(&target, ABOUT).await, wake(&source, ABOUT).await);
    assert_eq!(wake(&target, "project:here").await, here_before);

    // The source moves on; the destination's copy is now an exact prefix.
    source
        .service()
        .ingest(write(ABOUT, "shared:2", "second", "Ship the importer."))
        .await
        .expect("source grows");
    let report = target
        .store()
        .import_abouts(
            &read_only_bundle(source_dir.path(), &requested).await,
            &requested,
            projection_mutations_for_context_event,
        )
        .await
        .expect("extend");
    assert_eq!(report.abouts[ABOUT], AboutImportOutcome::Extended);
    assert_eq!(report.events_imported, 1);
    let imported = wake(&target, ABOUT).await;
    assert_eq!(imported, wake(&source, ABOUT).await);
    assert!(
        imported
            .iter()
            .any(|seen| seen == "project:shared:decision:second"),
        "{imported:?}"
    );

    // What the import projected is what a rebuild from the log projects.
    target
        .store()
        .rebuild_projections(projection_mutations_for_context_event)
        .await
        .expect("rebuild");
    assert_eq!(wake(&target, ABOUT).await, imported);
    assert_eq!(wake(&target, "project:here").await, here_before);

    // And the destination keeps accepting writes to the imported about.
    target
        .service()
        .ingest(write(ABOUT, "shared:3", "third", "Written here after."))
        .await
        .expect("imported about stays writable");
}
