#[path = "support/proof_snapshot.rs"]
mod support;

use std::sync::atomic::Ordering;
use std::time::Duration;

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{DimensionSelection, NodeDetailReader, ProjectionWriter};
use support::{Reads, command, command_in, inspect_query, service, temporal_query};

#[tokio::test]
async fn opening_a_snapshot_pins_even_an_empty_store_before_the_first_port_call() {
    let dir = tempfile::tempdir().expect("temporary store");
    let live = EmbeddedKernelStore::open(dir.path()).expect("open store");
    let view = live.read_snapshot().await.expect("pin empty snapshot");
    service(Reads::new(live.clone()), true)
        .ingest(command(1, "v1"))
        .await
        .expect("write after opening the view");
    assert!(
        view.load_node_detail(support::CLAIM)
            .await
            .expect("read old view")
            .is_none()
    );
    assert!(
        live.load_node_detail(support::CLAIM)
            .await
            .expect("read live store")
            .is_some()
    );
}

#[tokio::test]
async fn inspect_keeps_object_links_and_shared_sources_at_one_state() {
    for snapshot in [false, true] {
        let dir = tempfile::tempdir().expect("fixture operation succeeds");
        let reads =
            Reads::new(EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds"));
        let app = service(reads.clone(), snapshot);
        app.ingest(command(2, "v1"))
            .await
            .expect("fixture operation succeeds");
        let before = app
            .inspect(inspect_query())
            .await
            .expect("fixture operation succeeds");
        reads.pause.store(true, Ordering::SeqCst);
        let task_app = app.clone();
        let pending = tokio::spawn(async move {
            task_app
                .inspect(inspect_query())
                .await
                .expect("fixture operation succeeds")
        });
        tokio::time::timeout(Duration::from_secs(5), reads.paused.notified())
            .await
            .expect("fixture operation succeeds");
        let peer = service(
            Reads::new(EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds")),
            true,
        );
        peer.ingest(command(2, "v2"))
            .await
            .expect("fixture operation succeeds");
        reads.resume.notify_one();
        let during = pending.await.expect("fixture operation succeeds");
        let after = app
            .inspect(inspect_query())
            .await
            .expect("fixture operation succeeds");
        assert!(before != after);
        if snapshot {
            assert!(
                during == before,
                "snapshot must keep the complete earlier proof"
            );
            assert_eq!(reads.opened.load(Ordering::SeqCst), 3);
        } else {
            assert!(
                during != before && during != after,
                "control must reproduce the split-port bug"
            );
            assert_eq!(during.detail.node.summary, before.detail.node.summary);
            assert!(during.detail.detail == after.detail.detail);
        }
    }
}

#[tokio::test]
async fn temporal_keeps_graph_bodies_and_all_about_selection_at_one_state() {
    for snapshot in [false, true] {
        let dir = tempfile::tempdir().expect("fixture operation succeeds");
        let reads =
            Reads::new(EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds"));
        let app = service(reads.clone(), snapshot);
        app.ingest(command(2, "v1"))
            .await
            .expect("fixture operation succeeds");
        app.ingest(command_in("project:foreign-proof", "v1"))
            .await
            .expect("fixture operation succeeds");
        let mut query = temporal_query();
        query.entry_selection = None;
        query.limit_entries = Some(10);
        query.dimensions = DimensionSelection::all().with_all_about_scope();
        let before = app
            .temporal(query.clone())
            .await
            .expect("fixture operation succeeds");
        reads.pause.store(true, Ordering::SeqCst);
        let task_app = app.clone();
        let task_query = query.clone();
        let pending = tokio::spawn(async move {
            task_app
                .temporal(task_query)
                .await
                .expect("fixture operation succeeds")
        });
        tokio::time::timeout(Duration::from_secs(5), reads.paused.notified())
            .await
            .expect("fixture operation succeeds");
        let peer = service(
            Reads::new(EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds")),
            true,
        );
        peer.ingest(command(2, "v2"))
            .await
            .expect("fixture operation succeeds");
        peer.ingest(command_in("project:foreign-proof", "v2"))
            .await
            .expect("fixture operation succeeds");
        reads.resume.notify_one();
        let during = pending.await.expect("fixture operation succeeds");
        let after = app
            .temporal(query)
            .await
            .expect("fixture operation succeeds");
        assert_ne!(before, after);
        if snapshot {
            assert_eq!(during, before);
            assert_eq!(reads.opened.load(Ordering::SeqCst), 3);
        } else {
            assert_ne!(during, before);
            assert_ne!(during, after);
        }
    }
}

#[tokio::test]
async fn snapshot_failure_never_falls_back_to_live_ports() {
    let dir = tempfile::tempdir().expect("fixture operation succeeds");
    let mut reads =
        Reads::new(EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds"));
    let writer = service(reads.clone(), false);
    writer
        .ingest(command(1, "v1"))
        .await
        .expect("fixture operation succeeds");
    reads.fail_snapshot = true;
    let reader = service(reads, true);
    let error = reader
        .inspect(inspect_query())
        .await
        .err()
        .expect("snapshot must fail");
    assert!(error.to_string().contains("deliberate snapshot refusal"));
}

#[tokio::test]
async fn snapshot_clones_are_read_only_and_return_a_clean_connection() {
    let dir = tempfile::tempdir().expect("fixture operation succeeds");
    let live = EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds");
    let app = service(Reads::new(live.clone()), true);
    app.ingest(command(1, "v1"))
        .await
        .expect("fixture operation succeeds");
    let snapshot = live
        .read_snapshot()
        .await
        .expect("fixture operation succeeds");
    let held = snapshot.clone();
    let before = snapshot
        .load_node_detail(support::CLAIM)
        .await
        .expect("fixture operation succeeds");
    assert!(
        snapshot
            .apply_mutations(vec![kmp_domain::ProjectionMutation::UpsertNodeDetail(
                before.clone().expect("fixture operation succeeds")
            )])
            .await
            .is_err()
    );
    app.ingest(command(1, "v2"))
        .await
        .expect("fixture operation succeeds");
    drop(snapshot);
    assert_eq!(
        held.load_node_detail(support::CLAIM)
            .await
            .expect("fixture operation succeeds"),
        before
    );
    drop(held);
    app.ingest(command(1, "v3"))
        .await
        .expect("fixture operation succeeds");
    let after = app
        .inspect(inspect_query())
        .await
        .expect("fixture operation succeeds");
    assert!(
        after
            .detail
            .detail
            .expect("fixture operation succeeds")
            .detail
            .starts_with("v3:")
    );
}

#[tokio::test]
async fn cancelled_read_releases_its_snapshot_without_harming_later_reads() {
    let dir = tempfile::tempdir().expect("fixture operation succeeds");
    let reads =
        Reads::new(EmbeddedKernelStore::open(dir.path()).expect("fixture operation succeeds"));
    let app = service(reads.clone(), true);
    app.ingest(command(1, "v1"))
        .await
        .expect("fixture operation succeeds");
    reads.pause.store(true, Ordering::SeqCst);
    let task_app = app.clone();
    let pending = tokio::spawn(async move { task_app.inspect(inspect_query()).await });
    tokio::time::timeout(Duration::from_secs(5), reads.paused.notified())
        .await
        .expect("fixture operation succeeds");
    pending.abort();
    assert!(matches!(pending.await, Err(error) if error.is_cancelled()));
    app.ingest(command(1, "v2"))
        .await
        .expect("fixture operation succeeds");
    let result = app
        .inspect(inspect_query())
        .await
        .expect("fixture operation succeeds");
    assert!(
        result
            .detail
            .detail
            .expect("fixture operation succeeds")
            .detail
            .starts_with("v2:")
    );
}
