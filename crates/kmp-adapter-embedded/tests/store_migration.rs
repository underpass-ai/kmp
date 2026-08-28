//! Retired-layout migration requests fail closed.
//!
//! KMP no longer links redb, so format-1 bytes must never reach a database
//! parser. The operator's only safe bridge is a portable export produced by
//! the last compatible release.

use std::fs;
use std::path::Path;

use kmp_adapter_embedded::{
    EmbeddedKernelStore, StorageEngine, format_version_path, legacy_redb_store_path,
};
use kmp_application::projection_mutations_for_context_event;

fn legacy_source(bytes: &[u8]) -> tempfile::TempDir {
    let source = tempfile::tempdir().expect("source tempdir");
    fs::write(format_version_path(source.path()), "1\n").expect("legacy stamp");
    let store = legacy_redb_store_path(source.path());
    fs::create_dir_all(store.parent().expect("store parent")).expect("store dir");
    fs::write(store, bytes).expect("legacy bytes");
    source
}

async fn rejected(source: &Path, destination: &Path) -> String {
    EmbeddedKernelStore::migrate_data_dir(
        source,
        destination,
        projection_mutations_for_context_event,
    )
    .await
    .expect_err("retired format must be rejected")
    .to_string()
}

#[tokio::test]
async fn a_truncated_format_1_store_is_rejected_without_parsing_or_mutation() {
    let source = legacy_source(b"truncated-redb-header");
    let store = legacy_redb_store_path(source.path());
    let before = fs::read(&store).expect("source before");
    let parent = tempfile::tempdir().expect("destination parent");
    let destination = parent.path().join("migrated");

    let message = rejected(source.path(), &destination).await;

    assert!(message.contains("contains no redb reader"), "{message}");
    assert!(message.contains("KMP 0.3.2"), "{message}");
    assert_eq!(fs::read(&store).expect("source after"), before);
    assert!(
        !destination.exists(),
        "rejection must not create scratch state"
    );
}

#[tokio::test]
async fn a_zero_length_format_1_store_can_never_report_success() {
    let source = legacy_source(&[]);
    let store = legacy_redb_store_path(source.path());
    let parent = tempfile::tempdir().expect("destination parent");
    let destination = parent.path().join("migrated");

    let message = rejected(source.path(), &destination).await;

    assert!(message.contains("retired format 1"), "{message}");
    assert_eq!(fs::metadata(store).expect("source metadata").len(), 0);
    assert!(
        !destination.exists(),
        "rejection must not create a destination"
    );
}

#[test]
fn opening_format_1_fails_before_touching_the_store() {
    let source = legacy_source(b"evidence");
    let store = legacy_redb_store_path(source.path());

    let message = EmbeddedKernelStore::open(source.path())
        .expect_err("retired store must not open")
        .to_string();

    assert!(message.contains("contains no redb reader"), "{message}");
    assert_eq!(fs::read(store).expect("source after"), b"evidence");
}

#[tokio::test]
async fn migration_refuses_a_source_that_is_its_own_destination() {
    let source = legacy_source(b"evidence");

    let message = EmbeddedKernelStore::migrate_data_dir(
        source.path(),
        source.path(),
        projection_mutations_for_context_event,
    )
    .await
    .expect_err("same directory must be refused")
    .to_string();

    assert!(message.contains("same data directory"), "{message}");
}

#[tokio::test]
async fn migration_refuses_a_source_newer_than_this_binary() {
    let source = tempfile::tempdir().expect("source tempdir");
    fs::write(format_version_path(source.path()), "99\n").expect("future stamp");
    let parent = tempfile::tempdir().expect("destination parent");
    let destination = parent.path().join("migrated");

    let message = rejected(source.path(), &destination).await;

    assert!(message.contains("upgrade the binary"), "{message}");
    assert!(!destination.exists());
}

#[tokio::test]
async fn format_2_does_not_need_store_migration() {
    let source = tempfile::tempdir().expect("source tempdir");
    let _store = EmbeddedKernelStore::open_with_engine(source.path(), StorageEngine::Sqlite)
        .expect("sqlite source");
    let parent = tempfile::tempdir().expect("destination parent");
    let destination = parent.path().join("migrated");

    let message = rejected(source.path(), &destination).await;

    assert!(message.contains("unnecessary and unsupported"), "{message}");
    assert!(!destination.exists());
}

#[tokio::test]
async fn open_or_migrate_reopens_an_existing_sqlite_destination() {
    let source = legacy_source(b"unread");
    let destination = tempfile::tempdir().expect("destination");
    drop(EmbeddedKernelStore::open(destination.path()).expect("sqlite destination"));

    let (_store, receipt) = EmbeddedKernelStore::open_or_migrate_data_dir(
        source.path(),
        destination.path(),
        projection_mutations_for_context_event,
    )
    .await
    .expect("existing destination reopens");

    assert!(receipt.is_none());
}

#[tokio::test]
async fn an_ordinary_sqlite_store_has_no_migration_receipt() {
    let directory = tempfile::tempdir().expect("data dir");
    let store = EmbeddedKernelStore::open(directory.path()).expect("store opens");

    assert!(
        store
            .migration_receipt()
            .await
            .expect("receipt reads")
            .is_none()
    );
}
