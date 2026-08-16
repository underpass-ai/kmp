//! ADR-018's acceptance scenario, and the reason a second engine exists: two
//! OS processes open the same data directory and both write, and afterwards
//! the store holds every event either of them was told was committed.
//!
//! On redb this is exactly the failure the ADR describes — the second
//! process cannot open the store — and the test says so on purpose, so the
//! difference between the engines is pinned by a test rather than by prose.
//! If redb ever grows multi-process support, that assertion is the one to
//! flip, and it will fail loudly the day it should.

use std::path::Path;
use std::process::{Command, Stdio};

use kmp_adapter_embedded::{EmbeddedKernelStore, StorageEngine};
#[cfg(feature = "sqlite")]
use kmp_domain::ContextEventStore;

const EVENTS_PER_WRITER: u64 = 200;
#[cfg(feature = "sqlite")]
const ROLE: &str = "memory";

fn spawn_writer(data_dir: &Path, about: &str) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_embedded_crash_writer"))
        .arg(data_dir)
        .arg(EVENTS_PER_WRITER.to_string())
        .arg(about)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("writer should spawn")
}

/// Runs two writers concurrently and returns how many exited successfully.
fn run_two_writers(data_dir: &Path) -> usize {
    let first = spawn_writer(data_dir, "shared:writer-a");
    let second = spawn_writer(data_dir, "shared:writer-b");
    [first, second]
        .into_iter()
        .map(|child| child.wait_with_output().expect("writer exits"))
        .filter(|output| output.status.success())
        .count()
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn two_processes_write_the_same_sqlite_store_and_no_event_is_lost() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    drop(
        EmbeddedKernelStore::open_with_engine(data_dir.path(), StorageEngine::Sqlite)
            .expect("stamp the directory for sqlite"),
    );

    let succeeded = run_two_writers(data_dir.path());
    assert_eq!(
        succeeded, 2,
        "both writers must complete on the sqlite engine"
    );

    let store = EmbeddedKernelStore::open(data_dir.path()).expect("store reopens");
    let (log_length, last_sequence) = store.event_log_stats().await.expect("log stats");
    assert_eq!(
        log_length,
        2 * EVENTS_PER_WRITER,
        "every event either writer committed must be in the log"
    );
    assert_eq!(
        last_sequence,
        2 * EVENTS_PER_WRITER,
        "sequences must be contiguous across both writers: no gaps, no duplicates"
    );
    for about in ["shared:writer-a", "shared:writer-b"] {
        let revision = store
            .current_revision(about, ROLE)
            .await
            .expect("revision reads");
        assert_eq!(
            revision, EVENTS_PER_WRITER,
            "each writer's aggregate must have advanced by exactly its own events"
        );
    }
}

/// The same scenario one step earlier: the store file does not exist yet.
///
/// Switching a database into WAL takes a brief exclusive lock, so two
/// processes creating the store at the same instant collide *there* — before
/// WAL is in effect for either of them, and `busy_timeout` is not consulted
/// for that collision. The test above cannot see this: it stamps the data
/// directory first, which creates the file and leaves it in WAL, so both
/// writers take the no-op path.
#[cfg(feature = "sqlite")]
#[tokio::test]
async fn two_processes_creating_the_same_sqlite_store_at_once_both_get_in() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    drop(
        EmbeddedKernelStore::open_with_engine(data_dir.path(), StorageEngine::Sqlite)
            .expect("stamp the directory for sqlite"),
    );

    // Keep the stamp, drop the database: whoever opens first has to create
    // it, and that is the window under test.
    let store_dir = data_dir.path().join("store");
    for suffix in ["", "-wal", "-shm"] {
        let file = store_dir.join(format!("kernel.sqlite3{suffix}"));
        if file.exists() {
            std::fs::remove_file(&file).expect("clear the store file");
        }
    }

    let succeeded = run_two_writers(data_dir.path());
    assert_eq!(
        succeeded, 2,
        "both writers must get in even when neither found a store to open"
    );

    let store = EmbeddedKernelStore::open(data_dir.path()).expect("store reopens");
    let (log_length, last_sequence) = store.event_log_stats().await.expect("log stats");
    assert_eq!(
        log_length,
        2 * EVENTS_PER_WRITER,
        "every event either writer committed must be in the log"
    );
    assert_eq!(
        last_sequence,
        2 * EVENTS_PER_WRITER,
        "sequences must be contiguous across both writers: no gaps, no duplicates"
    );
}

#[tokio::test]
async fn on_redb_the_second_process_is_refused_and_nothing_is_lost() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    drop(
        EmbeddedKernelStore::open_with_engine(data_dir.path(), StorageEngine::Redb)
            .expect("stamp the directory for redb"),
    );

    let succeeded = run_two_writers(data_dir.path());
    assert_eq!(
        succeeded, 1,
        "redb is single-process (ADR-011): exactly one writer holds the store"
    );

    // The one that got in must have lost nothing to the one that did not.
    let store = EmbeddedKernelStore::open(data_dir.path()).expect("store reopens");
    let (log_length, _) = store.event_log_stats().await.expect("log stats");
    assert_eq!(
        log_length, EVENTS_PER_WRITER,
        "the surviving writer's events are all there, and only those"
    );
}
