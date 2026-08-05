#![cfg(feature = "container-tests")]

mod support;

use std::sync::Arc;
use std::time::SystemTime;

use rehydration_adapter_nats::NatsContextEventStore;
use rehydration_domain::{ContextEventChange, ContextEventStore, ContextUpdatedEvent, PortError};
use support::nats_container::{NATS_INTERNAL_PORT, connect_with_retry, start_nats_container};
use testcontainers::core::IntoContainerPort;

fn sample_event(
    root: &str,
    role: &str,
    revision: u64,
    idem_key: Option<&str>,
) -> ContextUpdatedEvent {
    ContextUpdatedEvent {
        root_node_id: root.to_string(),
        role: role.to_string(),
        revision,
        content_hash: format!("hash-{revision}"),
        changes: vec![ContextEventChange {
            operation: "UPDATE".to_string(),
            entity_kind: "node".to_string(),
            entity_id: "node-1".to_string(),
            payload_json: "{}".to_string(),
            reason: None,
            scopes: vec![],
        }],
        idempotency_key: idem_key.map(str::to_string),
        logical_digest: None,
        requested_by: Some("test".to_string()),
        occurred_at: SystemTime::now(),
    }
}

#[tokio::test]
async fn append_and_read_revision() {
    let container = start_nats_container().await.expect("nats should start");
    let host_port = container
        .get_host_port_ipv4(NATS_INTERNAL_PORT.tcp())
        .await
        .expect("port should be mapped");
    let url = format!("nats://127.0.0.1:{host_port}");
    let client = connect_with_retry(&url).await.expect("nats should connect");

    let store = NatsContextEventStore::new(client, "test")
        .await
        .expect("store should initialize");

    assert_eq!(
        store
            .current_revision("node-1", "dev")
            .await
            .expect("should read"),
        0
    );

    let rev = store
        .append(sample_event("node-1", "dev", 1, None), 0)
        .await
        .expect("append should succeed");
    assert_eq!(rev, 1);

    assert_eq!(
        store
            .current_revision("node-1", "dev")
            .await
            .expect("should read"),
        1
    );

    let rev2 = store
        .append(sample_event("node-1", "dev", 2, None), 1)
        .await
        .expect("second append should succeed");
    assert_eq!(rev2, 2);
}

#[tokio::test]
async fn append_rejects_wrong_revision() {
    let container = start_nats_container().await.expect("nats should start");
    let host_port = container
        .get_host_port_ipv4(NATS_INTERNAL_PORT.tcp())
        .await
        .expect("port should be mapped");
    let url = format!("nats://127.0.0.1:{host_port}");
    let client = connect_with_retry(&url).await.expect("nats should connect");

    let store = NatsContextEventStore::new(client, "conflict")
        .await
        .expect("store should initialize");

    let err = store
        .append(sample_event("node-1", "dev", 1, None), 99)
        .await
        .expect_err("wrong revision should fail");

    assert!(matches!(err, PortError::Conflict(_)));
}

#[tokio::test]
async fn idempotency_key_deduplication() {
    let container = start_nats_container().await.expect("nats should start");
    let host_port = container
        .get_host_port_ipv4(NATS_INTERNAL_PORT.tcp())
        .await
        .expect("port should be mapped");
    let url = format!("nats://127.0.0.1:{host_port}");
    let client = connect_with_retry(&url).await.expect("nats should connect");

    let store = NatsContextEventStore::new(client, "idem")
        .await
        .expect("store should initialize");

    store
        .append(sample_event("node-1", "dev", 1, Some("key-1")), 0)
        .await
        .expect("first append should succeed");

    let outcome = store
        .find_by_idempotency_key("key-1")
        .await
        .expect("lookup should succeed");
    assert!(outcome.is_some());
    let outcome = outcome.expect("should find outcome");
    assert_eq!(outcome.revision, 1);
    assert_eq!(outcome.content_hash, "hash-1");

    assert!(
        store
            .find_by_idempotency_key("nonexistent")
            .await
            .expect("lookup should succeed")
            .is_none()
    );
}

#[tokio::test]
async fn content_hash_tracks_latest_event() {
    let container = start_nats_container().await.expect("nats should start");
    let host_port = container
        .get_host_port_ipv4(NATS_INTERNAL_PORT.tcp())
        .await
        .expect("port should be mapped");
    let url = format!("nats://127.0.0.1:{host_port}");
    let client = connect_with_retry(&url).await.expect("nats should connect");

    let store = NatsContextEventStore::new(client, "hash")
        .await
        .expect("store should initialize");

    assert!(
        store
            .current_content_hash("node-1", "dev")
            .await
            .expect("should read")
            .is_none(),
        "empty store should return None"
    );

    store
        .append(sample_event("node-1", "dev", 1, None), 0)
        .await
        .expect("append should succeed");

    let hash = store
        .current_content_hash("node-1", "dev")
        .await
        .expect("should read");
    assert_eq!(hash.as_deref(), Some("hash-1"));

    store
        .append(sample_event("node-1", "dev", 2, None), 1)
        .await
        .expect("second append should succeed");

    let hash2 = store
        .current_content_hash("node-1", "dev")
        .await
        .expect("should read");
    assert_eq!(hash2.as_deref(), Some("hash-2"));
}

#[tokio::test]
async fn concurrent_appends_one_wins_one_conflicts() {
    let container = start_nats_container().await.expect("nats should start");
    let host_port = container
        .get_host_port_ipv4(NATS_INTERNAL_PORT.tcp())
        .await
        .expect("port should be mapped");
    let url = format!("nats://127.0.0.1:{host_port}");

    // Two independent store instances sharing the same JetStream stream
    let client_a = connect_with_retry(&url).await.expect("connect a");
    let store_a = Arc::new(
        NatsContextEventStore::new(client_a, "race")
            .await
            .expect("store a"),
    );

    let client_b = connect_with_retry(&url).await.expect("connect b");
    let store_b = Arc::new(
        NatsContextEventStore::new(client_b, "race")
            .await
            .expect("store b"),
    );

    // Both read revision 0 and try to write revision 1 concurrently
    let sa = Arc::clone(&store_a);
    let sb = Arc::clone(&store_b);

    let task_a = tokio::spawn(async move {
        sa.append(sample_event("race-node", "dev", 1, None), 0)
            .await
    });
    let task_b = tokio::spawn(async move {
        sb.append(sample_event("race-node", "dev", 1, None), 0)
            .await
    });

    let (result_a, result_b) = tokio::join!(task_a, task_b);
    let result_a = result_a.expect("task a should not panic");
    let result_b = result_b.expect("task b should not panic");

    // Exactly one should succeed, the other should get Conflict
    let (winner, loser) = match (&result_a, &result_b) {
        (Ok(_), Err(_)) => (result_a, result_b),
        (Err(_), Ok(_)) => (result_b, result_a),
        (Ok(_), Ok(_)) => panic!("both appends succeeded — CAS is broken"),
        (Err(a), Err(b)) => panic!("both failed: a={a}, b={b}"),
    };

    assert_eq!(winner.expect("winner should be Ok"), 1);
    assert!(
        matches!(
            loser.expect_err("loser should be Err"),
            PortError::Conflict(_)
        ),
        "loser should get Conflict"
    );

    // Final revision should be 1 (only one write succeeded)
    assert_eq!(
        store_a
            .current_revision("race-node", "dev")
            .await
            .expect("should read"),
        1
    );
}
