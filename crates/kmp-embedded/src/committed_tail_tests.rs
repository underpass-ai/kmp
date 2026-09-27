//! A guarded write that begins where this process's last publish left the
//! store and the committed bundle is checked and published from their tails
//! (DESIGN L6, write in O(delta)); anything else moved sends it to the full
//! check, which still refuses a history that differs.

use std::time::{Duration, UNIX_EPOCH};

use kmp_domain::{ContextEventStore, ContextUpdatedEvent, PortError};

use crate::{CommitNativeBundle, EmbeddedKernel};

fn event(root: &str, revision: u64) -> ContextUpdatedEvent {
    ContextUpdatedEvent {
        root_node_id: root.to_string(),
        role: "agent".to_string(),
        revision,
        content_hash: format!("{root}:{revision}"),
        changes: Vec::new(),
        idempotency_key: Some(format!("{root}:{revision}")),
        logical_digest: None,
        requested_by: Some("committed-tail-test".to_string()),
        occurred_at: UNIX_EPOCH + Duration::from_secs(1_000 + revision),
    }
}

async fn append(kernel: &EmbeddedKernel, root: &str, revision: u64) {
    kernel
        .store()
        .append(event(root, revision), revision - 1)
        .await
        .expect("append");
}

/// One guarded write of one event; whether it began on the tail.
async fn guarded(
    native: &CommitNativeBundle,
    kernel: &EmbeddedKernel,
    root: &str,
    revision: u64,
) -> bool {
    let pending = native.begin_write(kernel.store()).await.expect("begin");
    let on_tail = pending.began_on_the_tail();
    append(kernel, root, revision).await;
    native
        .publish(kernel.store(), &pending)
        .await
        .expect("publish");
    pending.complete().expect("complete");
    on_tail
}

fn committed(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join(".kmp/memory.jsonl")).expect("bundle")
}

#[tokio::test]
async fn consecutive_writes_publish_from_the_tail_what_a_full_export_writes() {
    let dir = tempfile::tempdir().expect("dir");
    let kernel = EmbeddedKernel::open(&dir.path().join(".kernel")).expect("kernel");
    let native = CommitNativeBundle::new(
        dir.path().join(".kernel"),
        dir.path().join(".kmp/memory.jsonl"),
    );
    assert!(
        !guarded(&native, &kernel, "project:a", 1).await,
        "the first write checks in full"
    );
    for revision in 2..=4 {
        assert!(guarded(&native, &kernel, "project:a", revision).await);
        assert_eq!(
            committed(dir.path()),
            kernel.store().export_bundle().await.expect("export")
        );
    }
    // A second process guarding the same paths begins with the full check,
    // and agrees.
    let other = CommitNativeBundle::new(
        dir.path().join(".kernel"),
        dir.path().join(".kmp/memory.jsonl"),
    );
    assert!(!guarded(&other, &kernel, "project:b", 1).await);
    assert_eq!(
        committed(dir.path()),
        kernel.store().export_bundle().await.expect("export")
    );
}

#[tokio::test]
async fn excluded_abouts_stay_out_of_a_bundle_published_from_the_tail() {
    let dir = tempfile::tempdir().expect("dir");
    let kernel = EmbeddedKernel::open(&dir.path().join(".kernel")).expect("kernel");
    let excluded = vec!["guide:kmp".to_string()];
    let native = CommitNativeBundle::new_excluding_abouts(
        dir.path().join(".kernel"),
        dir.path().join(".kmp/memory.jsonl"),
        excluded.clone(),
    );
    guarded(&native, &kernel, "project:a", 1).await;
    // Release-owned content lands beside the guard, then a guarded write.
    append(&kernel, "guide:kmp", 1).await;
    let pending = native.begin_write(kernel.store()).await.expect("begin");
    assert!(
        !pending.began_on_the_tail(),
        "the store moved behind the guard"
    );
    pending.complete().expect("abandoned");
    assert!(!guarded(&native, &kernel, "project:a", 2).await);
    assert!(guarded(&native, &kernel, "project:a", 3).await);
    assert_eq!(
        committed(dir.path()),
        kernel
            .store()
            .export_bundle_excluding_abouts(&excluded)
            .await
            .expect("export")
    );
}

#[tokio::test]
async fn a_history_that_moved_behind_the_guard_is_refused_by_the_full_check() {
    let dir = tempfile::tempdir().expect("dir");
    let kernel = EmbeddedKernel::open(&dir.path().join(".kernel")).expect("kernel");
    let native = CommitNativeBundle::new(
        dir.path().join(".kernel"),
        dir.path().join(".kmp/memory.jsonl"),
    );
    guarded(&native, &kernel, "project:a", 1).await;
    guarded(&native, &kernel, "project:a", 2).await;
    // Someone writes the store without the guard: the bundle now lags it.
    append(&kernel, "project:a", 3).await;
    let refused = native
        .begin_write(kernel.store())
        .await
        .err()
        .expect("refused");
    assert!(matches!(refused, PortError::Conflict(_)), "{refused:?}");
}

#[tokio::test]
async fn a_bundle_rewritten_by_someone_else_is_refused_by_the_full_check() {
    let dir = tempfile::tempdir().expect("dir");
    let kernel = EmbeddedKernel::open(&dir.path().join(".kernel")).expect("kernel");
    let native = CommitNativeBundle::new(
        dir.path().join(".kernel"),
        dir.path().join(".kmp/memory.jsonl"),
    );
    guarded(&native, &kernel, "project:a", 1).await;
    guarded(&native, &kernel, "project:a", 2).await;
    // A checkout brings another branch of the same length.
    let other = tempfile::tempdir().expect("other");
    let branch = EmbeddedKernel::open(other.path()).expect("branch");
    append(&branch, "project:a", 1).await;
    append(&branch, "project:z", 1).await;
    let bundle = branch.store().export_bundle().await.expect("branch bundle");
    crate::write_bundle_atomically(&dir.path().join(".kmp/memory.jsonl"), &bundle)
        .expect("checkout");
    let refused = native
        .begin_write(kernel.store())
        .await
        .err()
        .expect("refused");
    assert!(
        matches!(refused, PortError::Conflict(_) | PortError::InvalidState(_)),
        "{refused:?}"
    );
}
