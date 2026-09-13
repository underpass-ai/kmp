use std::fs;
use std::time::SystemTime;

use kmp_adapter_embedded::{EmbeddedKernelStore, StorageEngine};
use kmp_domain::{ContextEventStore, ContextUpdatedEvent};

fn event(revision: u64) -> ContextUpdatedEvent {
    ContextUpdatedEvent {
        root_node_id: "test:sqlite-lifecycle".to_string(),
        role: "memory".to_string(),
        revision,
        content_hash: format!("hash-{revision}"),
        changes: Vec::new(),
        idempotency_key: None,
        logical_digest: None,
        requested_by: None,
        occurred_at: SystemTime::now(),
    }
}

#[tokio::test]
async fn pinned_reader_keeps_committed_snapshot_while_writer_advances() {
    let data_dir = tempfile::tempdir().expect("data dir");
    let store = EmbeddedKernelStore::open_with_engine(data_dir.path(), StorageEngine::Sqlite)
        .expect("store opens");
    store.append(event(1), 0).await.expect("first commit");

    let snapshot = store.read_snapshot().await.expect("snapshot opens");
    store.append(event(2), 1).await.expect("second commit");

    assert_eq!(
        snapshot.event_log_stats().await.expect("pinned log stats"),
        (1, 1),
        "reader retains the snapshot from before the writer commit"
    );
    assert_eq!(
        store.event_log_stats().await.expect("current log stats"),
        (2, 2),
        "the current writer sees its committed event"
    );
}

#[test]
fn corrupt_kernel_input_is_refused_before_use() {
    let data_dir = tempfile::tempdir().expect("data dir");
    fs::create_dir_all(data_dir.path().join("store")).expect("store directory");
    fs::write(
        data_dir.path().join("FORMAT_VERSION"),
        format!("{}\n", StorageEngine::Sqlite.format_version()),
    )
    .expect("format stamp");
    fs::write(data_dir.path().join("store/kernel.sqlite3"), b"not sqlite").expect("corrupt kernel");

    let error = EmbeddedKernelStore::open(data_dir.path())
        .expect_err("corrupt SQLite input must be refused");
    assert!(
        error.to_string().contains("embedded store") || error.to_string().contains("SQLite"),
        "error identifies the rejected SQLite input: {error}"
    );
}
