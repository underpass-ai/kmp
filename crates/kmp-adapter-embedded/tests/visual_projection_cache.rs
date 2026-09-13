#[allow(dead_code)]
#[path = "support/proof_snapshot.rs"]
mod support;

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_application::{VisualLevelOfDetail, VisualProjectionQuery, VisualProjectionResult};
use kmp_domain::*;
use std::{sync::atomic::Ordering, time::Duration};
use support::{ABOUT, CLAIM, Reads, Service, command, command_in, service};

fn query() -> VisualProjectionQuery {
    VisualProjectionQuery {
        about: ABOUT.into(),
        from: "2026-09-01T00:00:00Z".into(),
        to: "2026-10-01T00:00:00Z".into(),
        axis: TemporalAxis::Observed,
        dimensions: DimensionSelection::all(),
        level_of_detail: VisualLevelOfDetail::Moment,
        bin_count: 8,
        page_entries: 1,
        cursor: None,
        depth: 8,
    }
}

async fn check_hit(
    cached: &Service,
    reference: &Service,
    reads: &Reads,
    query: VisualProjectionQuery,
) -> VisualProjectionResult {
    let expected = reference
        .visual_projection(query.clone())
        .await
        .expect("uncached");
    assert_eq!(
        cached
            .visual_projection(query.clone())
            .await
            .expect("first"),
        expected
    );
    let graphs = reads.catalogue_reads.load(Ordering::SeqCst);
    let details = reads.detail_ids.lock().expect("counts").len();
    assert_eq!(
        cached.visual_projection(query).await.expect("hit"),
        expected
    );
    assert_eq!(
        reads.catalogue_reads.load(Ordering::SeqCst),
        graphs,
        "hit reads no graph"
    );
    assert_eq!(
        reads.detail_ids.lock().expect("counts").len(),
        details,
        "hit reads no bodies"
    );
    expected
}

#[tokio::test]
async fn identical_queries_reuse_exact_projections_for_all_clocks_lods_filters_and_pages() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let mut reads = Reads::new(store.clone());
    reads.cache_revisions = true;
    let cached = service(reads.clone(), true);
    let reference = service(Reads::new(store), true);
    cached.ingest(command(2, "v1")).await.expect("seed");
    cached
        .ingest(command_in("project:foreign-proof", "v1"))
        .await
        .expect("foreign seed");
    for axis in [
        TemporalAxis::Default,
        TemporalAxis::Occurred,
        TemporalAxis::Observed,
        TemporalAxis::Ingested,
        TemporalAxis::Validity,
    ] {
        for lod in [
            VisualLevelOfDetail::Atlas,
            VisualLevelOfDetail::Episode,
            VisualLevelOfDetail::Moment,
        ] {
            for dimensions in [
                DimensionSelection::all(),
                DimensionSelection::only(["task"]),
                DimensionSelection::except(["task"]),
                DimensionSelection::all().with_scope_ids(["task:proof"]),
                DimensionSelection::all().with_selectors([LabelSelector::new(
                    "task",
                    LabelSelectorOperator::NotExists,
                    Vec::<String>::new(),
                )
                .expect("selector")]),
                DimensionSelection::all().with_all_about_scope(),
                DimensionSelection::all().with_about_scope(["project:foreign-proof"]),
            ] {
                let mut q = query();
                q.axis = axis;
                q.level_of_detail = lod;
                q.dimensions = dimensions;
                loop {
                    let page = check_hit(&cached, &reference, &reads, q.clone()).await;
                    let Some(cursor) = page.page.next_cursor else {
                        break;
                    };
                    q.cursor = Some(cursor);
                }
            }
        }
    }
}

#[tokio::test]
async fn changed_query_fields_never_return_a_different_cached_selection() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let mut reads = Reads::new(store.clone());
    reads.cache_revisions = true;
    let cached = service(reads.clone(), true);
    let reference = service(Reads::new(store), true);
    cached.ingest(command(1, "v1")).await.expect("seed");
    cached
        .ingest(command_in("project:foreign-proof", "other"))
        .await
        .expect("foreign seed");
    let initial = check_hit(&cached, &reference, &reads, query()).await;
    for field in [
        "from",
        "to",
        "axis",
        "dimensions",
        "lod",
        "bins",
        "page",
        "depth",
        "about",
        "cursor",
    ] {
        let mut q = query();
        match field {
            "from" => q.from = "2026-09-10T11:00:00Z".into(),
            "to" => q.to = "2026-09-10T09:00:00Z".into(),
            "axis" => q.axis = TemporalAxis::Occurred,
            "dimensions" => q.dimensions = DimensionSelection::except(["task"]),
            "lod" => q.level_of_detail = VisualLevelOfDetail::Episode,
            "bins" => q.bin_count = 3,
            "page" => q.page_entries = 2,
            "depth" => q.depth = 1,
            "about" => q.about = "project:foreign-proof".into(),
            "cursor" => q.cursor = initial.page.next_cursor.clone(),
            _ => unreachable!(),
        }
        let before = reads.catalogue_reads.load(Ordering::SeqCst);
        check_hit(&cached, &reference, &reads, q).await;
        assert!(
            reads.catalogue_reads.load(Ordering::SeqCst) > before,
            "{field} is a miss"
        );
    }
    let mut invalid = query();
    invalid.cursor = Some("invalid".into());
    assert!(cached.visual_projection(invalid).await.is_err());
    let mut invalid = query();
    invalid.to = invalid.from.clone();
    assert!(cached.visual_projection(invalid).await.is_err());
}

#[tokio::test]
async fn peer_writes_labels_lifecycle_and_foreign_abouts_invalidate_even_historical_views() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let mut reads = Reads::new(store);
    reads.cache_revisions = true;
    let cached = service(reads.clone(), true);
    cached.ingest(command(1, "v1")).await.expect("seed");
    let peer_store = EmbeddedKernelStore::open(dir.path()).expect("peer");
    let peer = service(Reads::new(peer_store.clone()), true);
    let mut q = query();
    q.dimensions = DimensionSelection::all().with_all_about_scope();
    let before = check_hit(&cached, &peer, &reads, q.clone()).await;
    peer.ingest(command_in("project:foreign-proof", "v1"))
        .await
        .expect("new about");
    let foreign = check_hit(&cached, &peer, &reads, q.clone()).await;
    assert_ne!(before, foreign);
    peer.ingest(command_in("project:foreign-proof", "v2"))
        .await
        .expect("foreign edit");
    assert_ne!(foreign, check_hit(&cached, &peer, &reads, q.clone()).await);
    let historical = VisualProjectionQuery {
        to: "2026-09-10T09:00:00Z".into(),
        ..q.clone()
    };
    check_hit(&cached, &peer, &reads, historical.clone()).await;
    let old_page = cached.visual_projection(query()).await.expect("old page");
    let mut obsolete = query();
    obsolete.cursor = old_page.page.next_cursor;
    assert!(obsolete.cursor.is_some(), "exercise a real continuation");
    let old_tail = check_hit(&cached, &peer, &reads, obsolete.clone()).await;
    peer.ingest(command(1, "v2")).await.expect("local edit");
    // Preserve the existing cursor contract, including edits it admits. The
    // cache must still return the newly computed page, never the retained tail.
    let new_tail = check_hit(&cached, &peer, &reads, obsolete).await;
    assert_ne!(old_tail, new_tail);
    let graphs = reads.catalogue_reads.load(Ordering::SeqCst);
    check_hit(&cached, &peer, &reads, historical).await;
    assert!(reads.catalogue_reads.load(Ordering::SeqCst) > graphs);
    // Raw projection mutations bypass the ingest facade: revision observation
    // must catch them too, including an edited label's clocks and a relation.
    let scope = MemoryDimensionIdentity::new(ABOUT, "task", "task:proof")
        .expect("scope")
        .node_id();
    for mutation in [
        ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
            source_node_id: scope.clone(),
            target_node_id: CLAIM.into(),
            relation_type: "contains_entry".into(),
            explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                .with_dimension("task")
                .with_scope_id(scope.clone())
                .with_observed_at("2026-09-11T12:00:00Z")
                .with_valid_until("2026-09-12T00:00:00Z"),
        })),
        ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
            source_node_id: CLAIM.into(),
            target_node_id: format!("{ABOUT}:entry:observation:other"),
            relation_type: "supersedes".into(),
            explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                .with_rationale("The second fixture replaces the first.")
                .with_evidence("Observed replacement.")
                .with_observed_at("2026-09-11T12:00:00Z"),
        })),
        ProjectionMutation::RemoveNodeRelation {
            source_node_id: scope,
            target_node_id: CLAIM.into(),
            relation_type: "contains_entry".into(),
        },
        ProjectionMutation::UpdateNodeStatus {
            node_id: CLAIM.into(),
            status: "SUPERSEDED".into(),
        },
        ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
            node_id: CLAIM.into(),
            detail: "A replacement body with stable node metadata.".into(),
            content_hash: "raw-body-v3".into(),
            revision: 3,
        }),
    ] {
        let before = reads.catalogue_reads.load(Ordering::SeqCst);
        peer_store
            .apply_mutations(vec![mutation])
            .await
            .expect("raw peer mutation");
        check_hit(&cached, &peer, &reads, q.clone()).await;
        assert!(reads.catalogue_reads.load(Ordering::SeqCst) > before);
    }
}

#[tokio::test]
async fn in_flight_old_snapshot_never_poisons_a_new_revision() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let mut reads = Reads::new(store);
    reads.cache_revisions = true;
    let cached = service(reads.clone(), true);
    cached.ingest(command(1, "v1")).await.expect("seed");
    let peer = service(
        Reads::new(EmbeddedKernelStore::open(dir.path()).expect("peer")),
        true,
    );
    let old = peer.visual_projection(query()).await.expect("old oracle");
    reads.pause.store(true, Ordering::SeqCst);
    let pending_service = cached.clone();
    let pending = tokio::spawn(async move { pending_service.visual_projection(query()).await });
    tokio::time::timeout(Duration::from_secs(5), reads.paused.notified())
        .await
        .expect("paused");
    peer.ingest(command(1, "v2"))
        .await
        .expect("concurrent write");
    let current = check_hit(&cached, &peer, &reads, query()).await;
    assert_ne!(old, current);
    reads.resume.notify_one();
    assert_eq!(pending.await.expect("join").expect("old read"), old);
    assert_eq!(check_hit(&cached, &peer, &reads, query()).await, current);
}

#[tokio::test]
async fn unavailable_revisions_and_snapshot_errors_do_not_reuse_results() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let reads = Reads::new(store.clone());
    let uncached = service(reads.clone(), true);
    uncached.ingest(command(1, "v1")).await.expect("seed");
    uncached.visual_projection(query()).await.expect("first");
    let graphs = reads.catalogue_reads.load(Ordering::SeqCst);
    uncached.visual_projection(query()).await.expect("second");
    assert!(reads.catalogue_reads.load(Ordering::SeqCst) > graphs);
    let mut refused = Reads::new(store);
    refused.cache_revisions = true;
    refused.fail_snapshot = true;
    assert!(
        service(refused, true)
            .visual_projection(query())
            .await
            .is_err()
    );
}
