//! Opening a project store brings it and its committed bundle together when
//! the histories prove the answer, and leaves everything else untouched.

use std::time::{Duration, UNIX_EPOCH};

use kmp_domain::{ContextEventStore, ContextUpdatedEvent};

use crate::{CommitNativeBundle, EmbeddedKernel, OpenReconciliation, pending_bundle_exports};

fn event(root: &str, revision: u64, hash: &str) -> ContextUpdatedEvent {
    ContextUpdatedEvent {
        root_node_id: root.to_string(),
        role: "agent".to_string(),
        revision,
        content_hash: hash.to_string(),
        changes: Vec::new(),
        idempotency_key: Some(format!("{root}:{hash}")),
        logical_digest: None,
        requested_by: Some("open-reconciliation-test".to_string()),
        occurred_at: UNIX_EPOCH + Duration::from_secs(1_000 + revision),
    }
}

async fn append(kernel: &EmbeddedKernel, root: &str, revision: u64, hash: &str) {
    kernel
        .store()
        .append(event(root, revision, hash), revision - 1)
        .await
        .expect("append");
}

struct Project {
    dir: tempfile::TempDir,
}

impl Project {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("dir"),
        }
    }
    fn data_dir(&self) -> std::path::PathBuf {
        self.dir.path().join(".kernel")
    }
    fn bundle(&self) -> std::path::PathBuf {
        self.dir.path().join(".kmp/memory.jsonl")
    }
    fn native(&self) -> CommitNativeBundle {
        CommitNativeBundle::new(self.data_dir(), self.bundle())
    }
    fn kernel(&self) -> EmbeddedKernel {
        EmbeddedKernel::open(&self.data_dir()).expect("kernel")
    }
    fn mark_pending(&self) {
        let pending = self.data_dir().join(crate::PENDING_EXPORT_DIR);
        std::fs::create_dir_all(&pending).expect("pending dir");
        std::fs::write(pending.join("write-killed.pending"), b"bundle=x\n").expect("marker");
    }
}

#[tokio::test]
async fn a_fresh_clone_and_a_pull_are_caught_up() {
    let teammate = Project::new();
    let theirs = teammate.kernel();
    append(&theirs, "project:a", 1, "a1").await;
    append(&theirs, "project:a", 2, "a2").await;
    let bundle = theirs.store().export_bundle().await.expect("export");

    let clone = Project::new();
    crate::write_bundle_atomically(&clone.bundle(), &bundle).expect("clone bundle");
    let mine = clone.kernel();
    assert_eq!(
        clone
            .native()
            .reconcile_on_open(mine.store())
            .expect("open"),
        OpenReconciliation::FastForwarded {
            appended: 2,
            events: 2
        }
    );
    let mine_now = mine.store().export_bundle().await.expect("export");
    assert_eq!(
        crate::verify_bundle(&mine_now)
            .expect("verify")
            .content_digest,
        crate::verify_bundle(&bundle)
            .expect("verify")
            .content_digest,
        "the clone holds the teammate's history exactly"
    );

    append(&theirs, "project:a", 3, "a3").await;
    let pulled = theirs.store().export_bundle().await.expect("export");
    crate::write_bundle_atomically(&clone.bundle(), &pulled).expect("pull");
    // A write in a session opened before the pull catches up first.
    let pending = clone
        .native()
        .begin_write(mine.store())
        .await
        .expect("guarded write after a pull");
    pending.complete().expect("complete");
    assert_eq!(
        crate::verify_bundle(&mine.store().export_bundle().await.expect("export"))
            .expect("verify")
            .event_count,
        3
    );
}

#[tokio::test]
async fn an_interrupted_write_is_published_and_its_marker_cleared() {
    let project = Project::new();
    let kernel = project.kernel();
    append(&kernel, "project:a", 1, "a1").await;
    crate::write_bundle_atomically(
        &project.bundle(),
        &kernel.store().export_bundle().await.expect("export"),
    )
    .expect("bundle");
    // The write committed to SQLite and the process died before publishing.
    append(&kernel, "project:a", 2, "a2").await;
    project.mark_pending();

    assert_eq!(
        project
            .native()
            .reconcile_on_open(kernel.store())
            .expect("open"),
        OpenReconciliation::Recovered { events: 2 }
    );
    assert!(pending_bundle_exports(&project.data_dir()).is_empty());
    assert_eq!(
        std::fs::read_to_string(project.bundle()).expect("bundle"),
        kernel.store().export_bundle().await.expect("export")
    );
}

#[tokio::test]
async fn a_first_write_killed_before_its_first_publish_is_recovered() {
    let project = Project::new();
    let kernel = project.kernel();
    append(&kernel, "project:a", 1, "a1").await;
    project.mark_pending();

    assert_eq!(
        project
            .native()
            .reconcile_on_open(kernel.store())
            .expect("open"),
        OpenReconciliation::Recovered { events: 1 }
    );
    assert!(project.bundle().is_file());
}

#[tokio::test]
async fn a_divergence_or_an_unmarked_store_ahead_is_left_alone() {
    let project = Project::new();
    let kernel = project.kernel();
    append(&kernel, "project:a", 1, "mine").await;
    let other = Project::new();
    let theirs = other.kernel();
    append(&theirs, "project:a", 1, "theirs").await;
    let bundle = theirs.store().export_bundle().await.expect("export");
    crate::write_bundle_atomically(&project.bundle(), &bundle).expect("bundle");
    project.mark_pending();

    assert!(matches!(
        project
            .native()
            .reconcile_on_open(kernel.store())
            .expect("open"),
        OpenReconciliation::Diverged(_)
    ));
    assert_eq!(pending_bundle_exports(&project.data_dir()).len(), 1);
    assert_eq!(
        std::fs::read_to_string(project.bundle()).expect("bundle"),
        bundle
    );

    let ahead = Project::new();
    let store = ahead.kernel();
    append(&store, "project:a", 1, "a1").await;
    crate::write_bundle_atomically(
        &ahead.bundle(),
        &store.store().export_bundle().await.expect("export"),
    )
    .expect("bundle");
    append(&store, "project:a", 2, "a2").await;
    assert!(matches!(
        ahead
            .native()
            .reconcile_on_open(store.store())
            .expect("open"),
        OpenReconciliation::StoreAhead(_)
    ));
}
