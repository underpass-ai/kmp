//! Store migration: the way out of the fail-fast rule.
//!
//! ADR-012 refuses to open a store whose `FORMAT_VERSION` this binary does
//! not support, so that memory is never silently read as empty. These tests
//! cover the escape hatch and, more importantly, its guarantees: the source
//! survives untouched, the destination cannot be an existing store, and the
//! result carries a receipt.

use std::fs;
use std::path::Path;

use kmp_adapter_embedded::{EmbeddedKernelStore, format_version_path};
use kmp_application::projection_mutations_for_context_event;
use kmp_domain::{ContextEventStore, NodeDetailReader};

const SEEDED_EVENTS: u64 = 25;

/// Seeds a data directory with real history, written by a separate process
/// so no handle is left open on it.
fn seeded_source(events: u64) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp source dir");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_embedded_crash_writer"))
        .arg(dir.path())
        .arg(events.to_string())
        .stdout(std::process::Stdio::null())
        .status()
        .expect("writer runs");
    assert!(status.success(), "seed writer must succeed");
    dir
}

fn store_file(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("store").join("kernel.redb")
}

fn digest(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(fs::read(path).expect("read store file"));
    format!("{:x}", hasher.finalize())
}

async fn migrate(source: &Path, destination: &Path) -> kmp_adapter_embedded::StoreMigrationReceipt {
    let (store, receipt) = EmbeddedKernelStore::migrate_data_dir(
        source,
        destination,
        projection_mutations_for_context_event,
    )
    .await
    .expect("migration succeeds");
    drop(store);
    receipt
}

#[tokio::test]
async fn migration_moves_history_and_rebuilds_what_it_derives() {
    let source = seeded_source(SEEDED_EVENTS);
    let destination = tempfile::tempdir().expect("temp destination dir");
    let destination_path = destination.path().join("migrated");

    let receipt = migrate(source.path(), &destination_path).await;

    assert_eq!(receipt.events_migrated, SEEDED_EVENTS);
    assert_eq!(receipt.source_format, 1);
    assert_eq!(receipt.destination_format, 1);
    assert!(
        receipt.mutations_applied > 0,
        "projections must be rebuilt, not copied"
    );
    assert_eq!(receipt.kernel_version, env!("CARGO_PKG_VERSION"));

    let store = EmbeddedKernelStore::open(&destination_path).expect("migrated store opens");
    let revision = store
        .current_revision("crash:test", "memory")
        .await
        .expect("revision reads");
    assert_eq!(revision, SEEDED_EVENTS, "history must arrive complete");
    let detail = store
        .load_node_detail(&format!("claim:{SEEDED_EVENTS:06}"))
        .await
        .expect("detail reads")
        .expect("last claim exists");
    assert_eq!(detail.revision, SEEDED_EVENTS);
}

#[tokio::test]
async fn migration_leaves_its_source_byte_for_byte() {
    let source = seeded_source(SEEDED_EVENTS);
    let destination = tempfile::tempdir().expect("temp destination dir");
    let destination_path = destination.path().join("migrated");

    let before = digest(&store_file(source.path()));
    let stamp_before = fs::read_to_string(format_version_path(source.path())).expect("stamp");

    let receipt = migrate(source.path(), &destination_path).await;

    let after = digest(&store_file(source.path()));
    assert_eq!(before, after, "the source store must not be written to");
    assert_eq!(
        receipt.source_sha256, before,
        "the receipt must name what it read"
    );
    assert_eq!(
        stamp_before,
        fs::read_to_string(format_version_path(source.path())).expect("stamp"),
        "the source stamp must not move"
    );
    assert!(
        !destination_path.join("migration-source.redb").exists(),
        "the scratch copy must not outlive the migration"
    );
}

#[tokio::test]
async fn a_store_this_binary_refuses_to_open_can_still_be_migrated() {
    let source = seeded_source(SEEDED_EVENTS);
    // Stamp it as an older format: exactly the state that makes the kernel
    // refuse to open the directory at all.
    fs::write(format_version_path(source.path()), "0\n").expect("restamp");

    let refusal = EmbeddedKernelStore::open(source.path())
        .expect_err("an older format must not open in place");
    assert!(
        refusal.to_string().contains("kmp-mcp migrate"),
        "the refusal must name the way out, got `{refusal}`"
    );

    let destination = tempfile::tempdir().expect("temp destination dir");
    let destination_path = destination.path().join("migrated");
    let receipt = migrate(source.path(), &destination_path).await;

    assert_eq!(receipt.source_format, 0);
    assert_eq!(receipt.destination_format, 1);
    let store = EmbeddedKernelStore::open(&destination_path).expect("migrated store opens");
    assert_eq!(
        store
            .current_revision("crash:test", "memory")
            .await
            .expect("revision reads"),
        SEEDED_EVENTS,
        "the memory that could not be opened is the memory that arrives"
    );
}

#[tokio::test]
async fn migration_refuses_to_write_over_existing_memory() {
    let source = seeded_source(2);
    let occupied = seeded_source(2);

    let error = EmbeddedKernelStore::migrate_data_dir(
        source.path(),
        occupied.path(),
        projection_mutations_for_context_event,
    )
    .await
    .expect_err("an occupied destination must be refused");
    assert!(
        error.to_string().contains("already holds a store"),
        "got `{error}`"
    );
}

#[tokio::test]
async fn re_running_a_finished_migration_says_so_instead_of_alarming() {
    let source = seeded_source(3);
    let destination = tempfile::tempdir().expect("temp destination dir");
    let destination_path = destination.path().join("migrated");

    migrate(source.path(), &destination_path).await;

    let error = EmbeddedKernelStore::migrate_data_dir(
        source.path(),
        &destination_path,
        projection_mutations_for_context_event,
    )
    .await
    .expect_err("a second migration must not run");
    assert!(
        error
            .to_string()
            .contains("already migrated from this exact source"),
        "a re-run deserves the truth, not a scare; got `{error}`"
    );
}

#[tokio::test]
async fn migration_refuses_a_source_that_is_its_own_destination() {
    let source = seeded_source(2);

    let error = EmbeddedKernelStore::migrate_data_dir(
        source.path(),
        source.path(),
        projection_mutations_for_context_event,
    )
    .await
    .expect_err("same directory must be refused");
    assert!(
        error.to_string().contains("same data directory"),
        "got `{error}`"
    );
}

#[tokio::test]
async fn migration_refuses_a_source_newer_than_this_binary() {
    let source = seeded_source(2);
    fs::write(format_version_path(source.path()), "99\n").expect("restamp");
    let destination = tempfile::tempdir().expect("temp destination dir");

    let error = EmbeddedKernelStore::migrate_data_dir(
        source.path(),
        &destination.path().join("migrated"),
        projection_mutations_for_context_event,
    )
    .await
    .expect_err("a newer source must be refused");
    assert!(
        error.to_string().contains("upgrade the binary"),
        "got `{error}`"
    );
}

#[tokio::test]
async fn open_or_migrate_runs_once_and_reopens_after() {
    let source = seeded_source(SEEDED_EVENTS);
    let destination = tempfile::tempdir().expect("temp destination dir");
    let destination_path = destination.path().join("migrated");

    let (first_store, first) = EmbeddedKernelStore::open_or_migrate_data_dir(
        source.path(),
        &destination_path,
        projection_mutations_for_context_event,
    )
    .await
    .expect("first call migrates");
    drop(first_store);
    let first = first.expect("first call reports a receipt");
    assert_eq!(first.events_migrated, SEEDED_EVENTS);

    let (second_store, second) = EmbeddedKernelStore::open_or_migrate_data_dir(
        source.path(),
        &destination_path,
        projection_mutations_for_context_event,
    )
    .await
    .expect("second call reopens");
    let second = second.expect("the receipt survives the reopen");
    assert_eq!(second, first, "a reopen must report the original migration");
    assert_eq!(
        second_store
            .current_revision("crash:test", "memory")
            .await
            .expect("revision reads"),
        SEEDED_EVENTS,
        "reopening must not replay history a second time"
    );
}

#[tokio::test]
async fn a_store_nobody_migrated_has_no_receipt() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store opens");

    assert!(
        store
            .migration_receipt()
            .await
            .expect("receipt reads")
            .is_none(),
        "an ordinary store must not claim a migration"
    );
}
