//! Compaction reclaims space and never loses data.

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_application::projection_mutations_for_context_event;
use kmp_domain::{ContextEventStore, NodeDetailReader};

#[tokio::test]
async fn compaction_after_rebuild_keeps_the_store_readable() {
    let data_dir = tempfile::tempdir().expect("temp data dir");

    let status = std::process::Command::new(env!("CARGO_BIN_EXE_embedded_crash_writer"))
        .arg(data_dir.path())
        .arg("200")
        .stdout(std::process::Stdio::null())
        .status()
        .expect("writer runs");
    assert!(status.success());

    {
        let store = EmbeddedKernelStore::open(data_dir.path()).expect("store opens");
        let report = store
            .rebuild_projections(projection_mutations_for_context_event)
            .await
            .expect("rebuild");
        assert_eq!(report.events_replayed, 200);
    }

    // All handles dropped: compaction takes exclusive ownership.
    EmbeddedKernelStore::compact_data_dir(data_dir.path()).expect("compaction runs");

    let store = EmbeddedKernelStore::open(data_dir.path()).expect("store reopens");
    let revision = store
        .current_revision("crash:test", "memory")
        .await
        .expect("revision reads");
    assert_eq!(revision, 200, "compaction must not lose events");
    let detail = store
        .load_node_detail("claim:000200")
        .await
        .expect("detail reads")
        .expect("last detail exists");
    assert_eq!(detail.revision, 200);
}
