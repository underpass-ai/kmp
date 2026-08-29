//! Compatibility tests for the retired in-process store-migration API.
//!
//! The current layout never migrates in process. Unsupported sources are
//! preserved for an external portable-bundle exporter, while an already-open
//! SQLite destination remains usable for downstream callers that still use
//! `open_or_migrate_data_dir`.

use kmp_adapter_embedded::{EmbeddedKernelStore, StorageEngine, format_version_path};

fn no_projection(
    _event: &kmp_domain::ContextUpdatedEvent,
) -> Result<Vec<kmp_domain::ProjectionMutation>, kmp_domain::PortError> {
    Ok(Vec::new())
}

#[tokio::test]
async fn unsupported_format_is_rejected_without_source_or_destination_mutation() {
    let source = tempfile::tempdir().expect("source");
    std::fs::write(format_version_path(source.path()), "1\n").expect("stamp");
    let artifact = source.path().join("store/retired-layout.bin");
    std::fs::create_dir_all(artifact.parent().expect("parent")).expect("store dir");
    std::fs::write(&artifact, b"historical bytes").expect("artifact");
    let parent = tempfile::tempdir().expect("destination parent");
    let destination = parent.path().join("destination");

    let error = EmbeddedKernelStore::migrate_data_dir(source.path(), &destination, no_projection)
        .await
        .expect_err("unsupported source must fail");

    let message = error.to_string();
    assert!(
        message.contains("unsupported format version 1"),
        "{message}"
    );
    assert!(
        message.contains("archived compatible exporter"),
        "{message}"
    );
    assert_eq!(
        std::fs::read(artifact).expect("source remains"),
        b"historical bytes"
    );
    assert!(!destination.exists());
}

#[tokio::test]
async fn current_sqlite_source_reports_that_migration_is_unnecessary() {
    let source = tempfile::tempdir().expect("source");
    let _store = EmbeddedKernelStore::open(source.path()).expect("SQLite source opens");
    let parent = tempfile::tempdir().expect("destination parent");
    let destination = parent.path().join("destination");

    let error = EmbeddedKernelStore::migrate_data_dir_to(
        source.path(),
        &destination,
        StorageEngine::Sqlite,
        no_projection,
    )
    .await
    .expect_err("current layout does not migrate");

    assert!(error.to_string().contains("unnecessary and unsupported"));
    assert!(!destination.exists());
}

#[tokio::test]
async fn source_and_destination_must_be_different() {
    let source = tempfile::tempdir().expect("source");
    let error = EmbeddedKernelStore::migrate_data_dir(source.path(), source.path(), no_projection)
        .await
        .expect_err("same directory must fail");
    assert!(
        error
            .to_string()
            .contains("source and destination are the same")
    );
}

#[tokio::test]
async fn open_or_migrate_reopens_an_existing_sqlite_destination() {
    let source = tempfile::tempdir().expect("unused source");
    let destination = tempfile::tempdir().expect("destination");
    drop(EmbeddedKernelStore::open(destination.path()).expect("destination opens"));

    let (_store, receipt) = EmbeddedKernelStore::open_or_migrate_data_dir(
        source.path(),
        destination.path(),
        no_projection,
    )
    .await
    .expect("existing destination reopens");

    assert!(receipt.is_none());
}
