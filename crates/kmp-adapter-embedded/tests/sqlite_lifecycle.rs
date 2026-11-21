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

/// A store leaves its proof of memory on the first append, whoever appended
/// (a session, `demo`, `import`, a project rebuild): losing the database
/// afterwards is refused instead of reopening empty. An opened but never
/// written store leaves none, so its database may be recreated.
#[tokio::test]
async fn the_first_append_leaves_a_proof_and_a_lost_database_is_then_refused() {
    let data_dir = tempfile::tempdir().expect("data dir");
    let proof = data_dir.path().join("held-memory");
    let database = data_dir.path().join("store/kernel.sqlite3");

    drop(EmbeddedKernelStore::open(data_dir.path()).expect("fresh store opens"));
    assert!(!proof.exists(), "an empty store holds no memory to prove");
    fs::remove_file(&database).expect("drop the empty database");
    let store = EmbeddedKernelStore::open(data_dir.path()).expect("an unwritten store reopens");

    store.append(event(1), 0).await.expect("first commit");
    assert!(proof.is_file(), "the first append leaves the proof");
    drop(store);

    fs::remove_file(&database).expect("lose the database");
    let error = EmbeddedKernelStore::open(data_dir.path())
        .expect_err("a store that held memory is not restarted empty")
        .to_string();
    assert!(error.contains("lost its database"), "{error}");
    assert!(error.contains("held-memory"), "{error}");
}

/// A store that already held events before proofs existed gets one on its
/// next writable open.
#[tokio::test]
async fn a_store_written_before_proofs_gets_one_on_open() {
    let data_dir = tempfile::tempdir().expect("data dir");
    let proof = data_dir.path().join("held-memory");
    let store = EmbeddedKernelStore::open(data_dir.path()).expect("store opens");
    store.append(event(1), 0).await.expect("first commit");
    drop(store);
    fs::remove_file(&proof).expect("as written by an older engine");

    drop(EmbeddedKernelStore::open(data_dir.path()).expect("store reopens"));
    assert!(proof.is_file(), "a writable open backfills the proof");
}
