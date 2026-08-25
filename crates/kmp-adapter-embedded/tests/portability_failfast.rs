//! Import fail-fast paths: every malformed bundle is rejected explicitly.

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_application::projection_mutations_for_context_event;

async fn expect_import_error(bundle: &str, needle: &str) {
    let dir = tempfile::tempdir().expect("dir");
    let store = EmbeddedKernelStore::open(dir.path()).expect("opens");
    let error = store
        .import_bundle(bundle, projection_mutations_for_context_event)
        .await
        .expect_err("malformed bundle must be rejected");
    assert!(
        error.to_string().contains(needle),
        "expected `{needle}` in `{error}`"
    );
}

#[tokio::test]
async fn empty_bundle_is_rejected() {
    expect_import_error("", "missing header").await;
}

#[tokio::test]
async fn unknown_bundle_format_is_rejected() {
    expect_import_error(
        r#"{"bundle_format":99,"store_format":1,"event_count":0,"kernel_version":"x"}"#,
        "bundle format 99",
    )
    .await;
}

#[tokio::test]
async fn mismatched_event_format_is_rejected() {
    expect_import_error(
        r#"{"bundle_format":1,"store_format":99,"event_count":0,"kernel_version":"x"}"#,
        "event format 99",
    )
    .await;
}

#[tokio::test]
async fn wrong_event_count_is_rejected() {
    expect_import_error(
        r#"{"bundle_format":1,"store_format":1,"event_count":3,"kernel_version":"x"}"#,
        "declares 3 events but 0",
    )
    .await;
}

#[tokio::test]
async fn corrupt_event_line_is_rejected() {
    let bundle = concat!(
        r#"{"bundle_format":1,"store_format":1,"event_count":1,"kernel_version":"x"}"#,
        "\n",
        "not-json"
    );
    expect_import_error(bundle, "could not decode bundle event").await;
}
