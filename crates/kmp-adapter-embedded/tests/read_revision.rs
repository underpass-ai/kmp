//! The revision a new read would stand on (P2 frozen continuations): equal to
//! the one a read reports while nothing is committed, moved by any commit,
//! including one from another process, and never certified without snapshots.

#[allow(dead_code)]
#[path = "support/proof_snapshot.rs"]
mod support;

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_application::WakeMemoryQuery;
use kmp_domain::{DimensionSelection, TemporalSelection};
use support::{Reads, command, command_in, service};

fn wake() -> WakeMemoryQuery {
    WakeMemoryQuery {
        about: support::ABOUT.into(),
        role: "agent".into(),
        intent: "resume".into(),
        dimensions: DimensionSelection::default(),
        token_budget: 1600,
        depth: 2,
        max_tier: None,
        max_entries: None,
        temporal: TemporalSelection::Frontier,
    }
}

#[tokio::test]
async fn the_read_revision_stands_until_any_commit() {
    let dir = tempfile::tempdir().expect("temporary store");
    let mut reads = Reads::new(EmbeddedKernelStore::open(dir.path()).expect("open store"));
    reads.cache_revisions = true;
    let app = service(reads, true);
    app.ingest(command(2, "v1")).await.expect("seed");

    let read = app.wake(wake()).await.expect("wake");
    let now = app.read_revision().await.expect("revision");
    assert!(now.is_some(), "a snapshot store certifies its revision");
    assert_eq!(read.read_revision, now, "a read reports the same revision");
    assert_eq!(app.read_revision().await.expect("revision"), now);

    // Another process commits to another about: the store moved on.
    let mut peer_reads = Reads::new(EmbeddedKernelStore::open(dir.path()).expect("peer store"));
    peer_reads.cache_revisions = true;
    service(peer_reads, true)
        .ingest(command_in("project:elsewhere", "v1"))
        .await
        .expect("peer write");
    let moved = app.read_revision().await.expect("revision");
    assert!(moved.is_some());
    assert_ne!(moved, now);
    assert_eq!(app.wake(wake()).await.expect("wake").read_revision, moved);
}

#[tokio::test]
async fn without_snapshots_no_revision_is_certified() {
    let dir = tempfile::tempdir().expect("temporary store");
    let mut reads = Reads::new(EmbeddedKernelStore::open(dir.path()).expect("open store"));
    reads.cache_revisions = true;
    let app = service(reads, false);
    assert_eq!(app.read_revision().await.expect("revision"), None);
}
