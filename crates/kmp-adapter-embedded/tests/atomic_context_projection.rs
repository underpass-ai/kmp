use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::{ContextEventStore, ContextUpdatedEvent, ProjectionMutation};

#[tokio::test]
async fn a_projection_failure_rolls_back_the_event_revision_and_receipt() {
    let directory = tempfile::tempdir().expect("directory");
    let store = EmbeddedKernelStore::open(directory.path()).expect("store");
    let event = ContextUpdatedEvent {
        root_node_id: "project:atomic".into(),
        role: "memory".into(),
        revision: 1,
        content_hash: "hash".into(),
        changes: vec![],
        idempotency_key: Some("atomic-write".into()),
        logical_digest: Some("logical".into()),
        requested_by: None,
        occurred_at: std::time::SystemTime::now(),
    };
    let error = store
        .append_projected(
            event.clone(),
            0,
            vec![],
            vec![ProjectionMutation::UpdateNodeStatus {
                node_id: "missing-node".into(),
                status: "ACTIVE".into(),
            }],
        )
        .await
        .expect_err("projection cannot update a missing node");
    assert!(error.to_string().contains("missing-node"));
    assert_eq!(store.event_log_stats().await.expect("log"), (0, 0));
    assert_eq!(
        store
            .current_revision("project:atomic", "memory")
            .await
            .expect("revision"),
        0
    );
    assert!(
        store
            .find_by_idempotency_key("atomic-write")
            .await
            .expect("receipt")
            .is_none()
    );
    // The failed transaction releases ownership and does not consume the key.
    assert_eq!(
        store
            .append_projected(event, 0, vec![], vec![])
            .await
            .expect("retry"),
        1
    );
    assert!(
        store
            .find_by_idempotency_key("atomic-write")
            .await
            .expect("receipt")
            .is_some()
    );
}
