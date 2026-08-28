//! ADR-018's acceptance scenario: two
//! OS processes open the same data directory and both write, and afterwards
//! the store holds every event either of them was told was committed.

use std::path::Path;
use std::process::{Command, Output, Stdio};

use kmp_adapter_embedded::{EmbeddedKernelStore, StorageEngine};
#[cfg(feature = "sqlite")]
use kmp_domain::ContextEventStore;

const CONTENTION_EVENTS_PER_WRITER: u64 = 200;
const FIRST_OPEN_EVENTS_PER_WRITER: u64 = 1;
#[cfg(feature = "sqlite")]
const ROLE: &str = "memory";

fn spawn_writer(data_dir: &Path, about: &str, event_count: u64) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_embedded_crash_writer"))
        .arg(data_dir)
        .arg(event_count.to_string())
        .arg(about)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("writer should spawn")
}

/// Runs two writers concurrently and preserves their output for failure
/// diagnostics. The first-open race is timing-sensitive, so dropping the
/// losing process's stderr turns a useful gate failure into only `1 != 2`.
fn run_two_writers(data_dir: &Path, event_count: u64) -> Vec<Output> {
    let first = spawn_writer(data_dir, "shared:writer-a", event_count);
    let second = spawn_writer(data_dir, "shared:writer-b", event_count);
    [first, second]
        .into_iter()
        .map(|child| child.wait_with_output().expect("writer exits"))
        .collect()
}

fn assert_both_writers_succeeded(outputs: &[Output], context: &str) {
    let failures = outputs
        .iter()
        .enumerate()
        .filter(|(_, output)| !output.status.success())
        .map(|(index, output)| {
            format!(
                "writer {}: status={}\nstdout:\n{}\nstderr:\n{}",
                index + 1,
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            )
        })
        .collect::<Vec<_>>();
    assert!(
        failures.is_empty(),
        "{context}; {} of {} writers failed:\n{}",
        failures.len(),
        outputs.len(),
        failures.join("\n\n"),
    );
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn two_processes_write_the_same_sqlite_store_and_no_event_is_lost() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    drop(
        EmbeddedKernelStore::open_with_engine(data_dir.path(), StorageEngine::Sqlite)
            .expect("stamp the directory for sqlite"),
    );

    let outputs = run_two_writers(data_dir.path(), CONTENTION_EVENTS_PER_WRITER);
    assert_both_writers_succeeded(&outputs, "both writers must complete on the sqlite engine");

    let store = EmbeddedKernelStore::open(data_dir.path()).expect("store reopens");
    let (log_length, last_sequence) = store.event_log_stats().await.expect("log stats");
    assert_eq!(
        log_length,
        2 * CONTENTION_EVENTS_PER_WRITER,
        "every event either writer committed must be in the log"
    );
    assert_eq!(
        last_sequence,
        2 * CONTENTION_EVENTS_PER_WRITER,
        "sequences must be contiguous across both writers: no gaps, no duplicates"
    );
    for about in ["shared:writer-a", "shared:writer-b"] {
        let revision = store
            .current_revision(about, ROLE)
            .await
            .expect("revision reads");
        assert_eq!(
            revision, CONTENTION_EVENTS_PER_WRITER,
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

    // One commit per process is enough to prove that both finished opening
    // and can use the freshly created store. The 200-commit contention
    // workload belongs to the test above; repeating it here made this gate
    // depend on runner fsync throughput instead of the first-open contract.
    let outputs = run_two_writers(data_dir.path(), FIRST_OPEN_EVENTS_PER_WRITER);
    assert_both_writers_succeeded(
        &outputs,
        "both writers must get in even when neither found a store to open",
    );

    let store = EmbeddedKernelStore::open(data_dir.path()).expect("store reopens");
    let (log_length, last_sequence) = store.event_log_stats().await.expect("log stats");
    assert_eq!(
        log_length,
        2 * FIRST_OPEN_EVENTS_PER_WRITER,
        "every event either writer committed must be in the log"
    );
    assert_eq!(
        last_sequence,
        2 * FIRST_OPEN_EVENTS_PER_WRITER,
        "sequences must be contiguous across both writers: no gaps, no duplicates"
    );
}
