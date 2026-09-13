#[allow(dead_code)]
#[path = "support/proof_snapshot.rs"]
mod support;

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{
    DimensionSelection, GraphNeighborhoodReader, TemporalAxis, TemporalCursor, TemporalDirection,
    TemporalInterval,
};
use std::sync::atomic::Ordering;
use support::{Reads, command, command_in, service, temporal_query};

#[tokio::test]
async fn indexed_pages_equal_uncached_reads_for_every_clock_and_selection() {
    let dir = tempfile::tempdir().expect("fixture operation succeeds");
    let store = EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds");
    let mut reads = Reads::new(store.clone());
    reads.cache_revisions = true;
    let cached = service(reads.clone(), true);
    let reference = service(Reads::new(store), true);
    cached
        .ingest(command(2, "v1"))
        .await
        .expect("fixture operation succeeds");
    for axis in [
        TemporalAxis::Default,
        TemporalAxis::Occurred,
        TemporalAxis::Observed,
        TemporalAxis::Ingested,
        TemporalAxis::Validity,
    ] {
        let mut query = temporal_query();
        query.axis = axis;
        query.entry_selection = None;
        cached
            .temporal(query.clone())
            .await
            .expect("fixture operation succeeds");
        let catalogue_reads = reads.catalogue_reads.load(Ordering::SeqCst);
        for direction in [
            TemporalDirection::Goto,
            TemporalDirection::Near,
            TemporalDirection::Rewind,
            TemporalDirection::Forward,
        ] {
            for dimensions in [
                DimensionSelection::all(),
                DimensionSelection::only(["task"]),
                DimensionSelection::except(["task"]),
            ] {
                query.direction = direction;
                query.dimensions = dimensions;
                query.cursor = Some(TemporalCursor::Time("2026-09-10T10:00:00Z".into()));
                let actual = cached
                    .temporal(query.clone())
                    .await
                    .expect("fixture operation succeeds");
                assert_eq!(
                    actual,
                    reference
                        .temporal(query.clone())
                        .await
                        .expect("fixture operation succeeds")
                );
            }
        }
        query.dimensions = DimensionSelection::all();
        query.direction = TemporalDirection::Forward;
        query.cursor = None;
        query.interval = Some(
            TemporalInterval::new(
                Some("2026-09-01T00:00:00Z".into()),
                Some("2026-10-01T00:00:00Z".into()),
            )
            .expect("fixture operation succeeds"),
        );
        query.limit_entries = Some(1);
        loop {
            let page = cached
                .temporal(query.clone())
                .await
                .expect("fixture operation succeeds");
            assert_eq!(
                page,
                reference
                    .temporal(query.clone())
                    .await
                    .expect("fixture operation succeeds")
            );
            let Some(next) = page.traversal.page().next_cursor() else {
                break;
            };
            query.cursor = Some(TemporalCursor::Ref(next.to_owned()));
        }
        assert_eq!(
            reads.catalogue_reads.load(Ordering::SeqCst),
            catalogue_reads,
            "same clock and roots reuse the full catalogue while selections change"
        );
    }
}

#[tokio::test]
async fn peer_edits_cross_about_changes_and_eviction_never_reuse_stale_catalogues() {
    let dir = tempfile::tempdir().expect("fixture operation succeeds");
    let mut reads =
        Reads::new(EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds"));
    reads.cache_revisions = true;
    let cached = service(reads.clone(), true);
    cached
        .ingest(command(2, "v1"))
        .await
        .expect("fixture operation succeeds");
    cached
        .ingest(command_in("project:foreign-proof", "v1"))
        .await
        .expect("fixture operation succeeds");
    let mut query = temporal_query();
    query.entry_selection = None;
    query.dimensions = DimensionSelection::all().with_all_about_scope();
    let before = cached
        .temporal(query.clone())
        .await
        .expect("fixture operation succeeds");
    let warmed = reads.catalogue_reads.load(Ordering::SeqCst);
    assert_eq!(
        cached
            .temporal(query.clone())
            .await
            .expect("fixture operation succeeds"),
        before
    );
    assert_eq!(reads.catalogue_reads.load(Ordering::SeqCst), warmed);
    let peer = service(
        Reads::new(EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds")),
        true,
    );
    peer.ingest(command_in("project:foreign-proof", "v2"))
        .await
        .expect("fixture operation succeeds");
    let after = cached
        .temporal(query.clone())
        .await
        .expect("fixture operation succeeds");
    assert_ne!(before, after);
    assert_eq!(
        after,
        peer.temporal(query.clone())
            .await
            .expect("fixture operation succeeds")
    );
    assert!(reads.catalogue_reads.load(Ordering::SeqCst) > warmed);
    // Different roots evict the sole retained index. Returning to the former
    // selection re-reads it rather than leaking another about's catalogue.
    let multi_reads = reads.catalogue_reads.load(Ordering::SeqCst);
    let mut single = query.clone();
    single.dimensions = DimensionSelection::all();
    cached
        .temporal(single)
        .await
        .expect("fixture operation succeeds");
    assert_eq!(
        cached
            .temporal(query)
            .await
            .expect("fixture operation succeeds"),
        after
    );
    assert!(reads.catalogue_reads.load(Ordering::SeqCst) >= multi_reads + 3);
}

#[tokio::test]
async fn revisions_observe_writes_without_any_new_binary_projection_hook() {
    let dir = tempfile::tempdir().expect("fixture operation succeeds");
    let store = EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds");
    service(Reads::new(store.clone()), true)
        .ingest(command(1, "v1"))
        .await
        .expect("fixture operation succeeds");
    assert!(
        store
            .graph_read_revision()
            .await
            .expect("fixture operation succeeds")
            .is_none(),
        "a live port certifies no snapshot"
    );
    let old = store
        .read_snapshot()
        .await
        .expect("fixture operation succeeds");
    let revision = old
        .graph_read_revision()
        .await
        .expect("fixture operation succeeds")
        .expect("fixture operation succeeds");
    assert_eq!(
        store
            .read_snapshot()
            .await
            .expect("fixture operation succeeds")
            .graph_read_revision()
            .await
            .expect("fixture operation succeeds"),
        Some(revision.clone())
    );
    // A raw SQLite commit models a peer or previous release which knows
    // nothing about this optimization. There is no shared counter to update.
    let peer = rusqlite::Connection::open(dir.path().join("store/kernel.sqlite3"))
        .expect("fixture operation succeeds");
    peer.execute_batch(
        "CREATE TABLE revision_probe(value INTEGER); INSERT INTO revision_probe VALUES(1)",
    )
    .expect("fixture operation succeeds");
    let current = store
        .read_snapshot()
        .await
        .expect("fixture operation succeeds");
    assert_ne!(
        current
            .graph_read_revision()
            .await
            .expect("fixture operation succeeds"),
        Some(revision.clone())
    );
    assert_eq!(
        old.graph_read_revision()
            .await
            .expect("fixture operation succeeds"),
        Some(revision)
    );
    let other_reader = EmbeddedKernelStore::open(dir.path())
        .expect("fixture operation succeeds")
        .read_snapshot()
        .await
        .expect("fixture operation succeeds");
    assert_ne!(
        current
            .graph_read_revision()
            .await
            .expect("fixture operation succeeds"),
        other_reader
            .graph_read_revision()
            .await
            .expect("fixture operation succeeds"),
        "data_version is never compared across observer connections"
    );
}
