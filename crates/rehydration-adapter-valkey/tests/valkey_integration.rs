use std::error::Error;
use std::time::{Duration, UNIX_EPOCH};

use rehydration_adapter_valkey::{
    ValkeyNodeDetailStore, ValkeyProcessedEventStore, ValkeyProjectionCheckpointStore,
    ValkeySnapshotStore,
};
use rehydration_domain::{
    BundleMetadata, BundleNode, BundleNodeDetail, CaseId, RehydrationBundle, Role,
    SnapshotSaveOptions,
};
use rehydration_ports::{
    NodeDetailProjection, NodeDetailReader, ProcessedEventStore, ProjectionCheckpoint,
    ProjectionCheckpointStore, ProjectionMutation, ProjectionWriter, SnapshotStore,
};
use rehydration_testkit::ensure_testcontainers_runtime;
use serde_json::{Value, json};
use testcontainers::{
    GenericImage,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const VALKEY_INTERNAL_PORT: u16 = 6379;

async fn start_valkey_container()
-> Result<testcontainers::ContainerAsync<GenericImage>, Box<dyn Error + Send + Sync>> {
    ensure_testcontainers_runtime()?;

    Ok(GenericImage::new("docker.io/valkey/valkey", "8.1.5-alpine")
        .with_exposed_port(VALKEY_INTERNAL_PORT.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
        .start()
        .await?)
}

#[tokio::test]
async fn save_bundle_persists_snapshot_in_valkey() -> Result<(), Box<dyn Error + Send + Sync>> {
    let container = start_valkey_container().await?;

    let host = container.get_host().await?;
    let port = container.get_host_port_ipv4(VALKEY_INTERNAL_PORT).await?;
    let address = format!("{host}:{port}");

    let store = ValkeySnapshotStore::new(format!(
        "redis://{address}?key_prefix=rehydration:it&ttl_seconds=120"
    ))?;

    let case_id = CaseId::new("node-123")?;
    let role = Role::new("reviewer")?;
    let bundle = RehydrationBundle::new(
        case_id.clone(),
        role.clone(),
        BundleNode::new(
            case_id.as_str(),
            "capability",
            format!("Node {}", case_id.as_str()),
            "expanded context",
            "ACTIVE",
            vec!["projection-node".to_string()],
            std::collections::BTreeMap::new(),
        ),
        Vec::new(),
        Vec::new(),
        vec![BundleNodeDetail::new(
            case_id.as_str(),
            "expanded context",
            "abc123",
            7,
        )],
        BundleMetadata {
            revision: 7,
            content_hash: "abc123".to_string(),
            generator_version: "integration-test".to_string(),
        },
    );
    let bundle = bundle?;

    store.save_bundle(&bundle).await?;

    let key = "rehydration:it:node-123:reviewer";
    let snapshot = send_command(&address, &["GET", key]).await?;
    let ttl = send_command(&address, &["TTL", key]).await?;

    match snapshot {
        RespValue::BulkString(Some(payload)) => {
            assert_eq!(
                serde_json::from_str::<Value>(&payload)?,
                json!({
                    "root_node_id": "node-123",
                    "role": "reviewer",
                    "root_node": {
                        "node_id": "node-123",
                        "node_kind": "capability",
                        "title": "Node node-123",
                        "summary": "expanded context",
                        "status": "ACTIVE",
                        "labels": ["projection-node"],
                        "properties": {}
                    },
                    "neighbor_nodes": [],
                    "relationships": [],
                    "node_details": [{
                        "node_id": "node-123",
                        "detail": "expanded context",
                        "content_hash": "abc123",
                        "revision": 7
                    }],
                    "stats": {
                        "selected_nodes": 1,
                        "selected_relationships": 0,
                        "detailed_nodes": 1
                    },
                    "metadata": {
                        "revision": 7,
                        "content_hash": "abc123",
                        "generator_version": "integration-test"
                    }
                })
            );
        }
        other => panic!("expected snapshot payload, got {other:?}"),
    }

    match ttl {
        RespValue::Integer(value) => {
            assert!((1..=120).contains(&value));
        }
        other => panic!("expected integer TTL response, got {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn save_bundle_respects_request_ttl_override() -> Result<(), Box<dyn Error + Send + Sync>> {
    let container = start_valkey_container().await?;

    let host = container.get_host().await?;
    let port = container.get_host_port_ipv4(VALKEY_INTERNAL_PORT).await?;
    let address = format!("{host}:{port}");

    let store = ValkeySnapshotStore::new(format!(
        "redis://{address}?key_prefix=rehydration:it&ttl_seconds=120"
    ))?;
    let case_id = CaseId::new("node-123")?;
    let role = Role::new("reviewer")?;
    let bundle = RehydrationBundle::new(
        case_id.clone(),
        role.clone(),
        BundleNode::new(
            case_id.as_str(),
            "capability",
            format!("Node {}", case_id.as_str()),
            "expanded context",
            "ACTIVE",
            vec!["projection-node".to_string()],
            std::collections::BTreeMap::new(),
        ),
        Vec::new(),
        Vec::new(),
        vec![BundleNodeDetail::new(
            case_id.as_str(),
            "expanded context",
            "abc123",
            7,
        )],
        BundleMetadata {
            revision: 7,
            content_hash: "abc123".to_string(),
            generator_version: "integration-test".to_string(),
        },
    )?;

    store
        .save_bundle_with_options(&bundle, SnapshotSaveOptions::new(Some(15)))
        .await?;

    match send_command(&address, &["TTL", "rehydration:it:node-123:reviewer"]).await? {
        RespValue::Integer(value) => assert!((1..=15).contains(&value)),
        other => panic!("expected integer TTL response, got {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn node_detail_roundtrip_reads_expanded_detail_from_valkey()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let container = start_valkey_container().await?;

    let host = container.get_host().await?;
    let port = container.get_host_port_ipv4(VALKEY_INTERNAL_PORT).await?;
    let address = format!("{host}:{port}");

    let store = ValkeyNodeDetailStore::new(format!(
        "redis://{address}?key_prefix=rehydration:detail&ttl_seconds=120"
    ))?;

    store
        .apply_mutations(vec![ProjectionMutation::UpsertNodeDetail(
            NodeDetailProjection {
                node_id: "node-123".to_string(),
                detail: "Expanded node detail".to_string(),
                content_hash: "hash-123".to_string(),
                revision: 3,
            },
        )])
        .await?;

    let loaded = store
        .load_node_detail("node-123")
        .await?
        .expect("detail should exist");
    let ttl = send_command(&address, &["TTL", "rehydration:detail:node-123"]).await?;

    assert_eq!(loaded.node_id, "node-123");
    assert_eq!(loaded.detail, "Expanded node detail");
    assert_eq!(loaded.content_hash, "hash-123");
    assert_eq!(loaded.revision, 3);

    match ttl {
        RespValue::Integer(value) => assert!((1..=120).contains(&value)),
        other => panic!("expected integer TTL response, got {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn processed_event_store_roundtrip_uses_real_valkey()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let container = start_valkey_container().await?;

    let host = container.get_host().await?;
    let port = container.get_host_port_ipv4(VALKEY_INTERNAL_PORT).await?;
    let address = format!("{host}:{port}");

    let store = ValkeyProcessedEventStore::new(format!(
        "redis://{address}?key_prefix=rehydration:processed"
    ))?;

    assert!(
        !store
            .has_processed("projection-consumer", "event-123")
            .await?
    );

    store
        .record_processed("projection-consumer", "event-123")
        .await?;

    assert!(
        store
            .has_processed("projection-consumer", "event-123")
            .await?
    );

    match send_command(
        &address,
        &["GET", "rehydration:processed:projection-consumer:event-123"],
    )
    .await?
    {
        RespValue::BulkString(Some(value)) => assert_eq!(value, "1"),
        other => panic!("expected processed event marker, got {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn projection_checkpoint_store_roundtrip_uses_real_valkey()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let container = start_valkey_container().await?;

    let host = container.get_host().await?;
    let port = container.get_host_port_ipv4(VALKEY_INTERNAL_PORT).await?;
    let address = format!("{host}:{port}");

    let store = ValkeyProjectionCheckpointStore::new(format!(
        "redis://{address}?key_prefix=rehydration:checkpoint"
    ))?;

    let checkpoint = ProjectionCheckpoint {
        consumer_name: "projection-consumer".to_string(),
        stream_name: "graph.node.materialized".to_string(),
        last_subject: "rehydration.graph.node.materialized".to_string(),
        last_event_id: "event-456".to_string(),
        last_correlation_id: "corr-456".to_string(),
        last_occurred_at: "2026-03-12T00:00:00Z".to_string(),
        processed_events: 5,
        updated_at: UNIX_EPOCH + Duration::from_secs(2),
    };

    store.save_checkpoint(checkpoint.clone()).await?;

    let loaded = store
        .load_checkpoint("projection-consumer", "graph.node.materialized")
        .await?
        .expect("checkpoint should exist");
    assert_eq!(loaded, checkpoint);

    match send_command(
        &address,
        &[
            "GET",
            "rehydration:checkpoint:projection-consumer:graph.node.materialized",
        ],
    )
    .await?
    {
        RespValue::BulkString(Some(payload)) => {
            let value: Value = serde_json::from_str(&payload)?;
            assert_eq!(value["last_event_id"], "event-456");
            assert_eq!(value["processed_events"], 5);
        }
        other => panic!("expected checkpoint payload, got {other:?}"),
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum RespValue {
    SimpleString(String),
    BulkString(Option<String>),
    Integer(i64),
    Error(String),
}

async fn send_command(
    address: &str,
    arguments: &[&str],
) -> Result<RespValue, Box<dyn Error + Send + Sync>> {
    let mut stream = TcpStream::connect(address).await?;
    let payload = encode_command(arguments);
    stream.write_all(&payload).await?;
    stream.flush().await?;

    let mut reader = BufReader::new(stream);
    read_response(&mut reader).await
}

fn encode_command(arguments: &[&str]) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(format!("*{}\r\n", arguments.len()).as_bytes());
    for argument in arguments {
        payload.extend_from_slice(format!("${}\r\n", argument.len()).as_bytes());
        payload.extend_from_slice(argument.as_bytes());
        payload.extend_from_slice(b"\r\n");
    }

    payload
}

async fn read_response(
    reader: &mut BufReader<TcpStream>,
) -> Result<RespValue, Box<dyn Error + Send + Sync>> {
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let line = line.trim_end_matches("\r\n");
    let (prefix, remainder) = line.split_at(1);

    match prefix {
        "+" => Ok(RespValue::SimpleString(remainder.to_string())),
        "-" => Ok(RespValue::Error(remainder.to_string())),
        ":" => Ok(RespValue::Integer(remainder.parse()?)),
        "$" => {
            let length: isize = remainder.parse()?;
            if length == -1 {
                return Ok(RespValue::BulkString(None));
            }

            let mut buffer = vec![0_u8; length as usize];
            reader.read_exact(&mut buffer).await?;
            let mut crlf = [0_u8; 2];
            reader.read_exact(&mut crlf).await?;
            Ok(RespValue::BulkString(Some(String::from_utf8(buffer)?)))
        }
        other => Err(format!("unsupported RESP prefix `{other}`").into()),
    }
}

// ── Event store CAS tests ──────────────────────────────────────────────

use rehydration_adapter_valkey::ValkeyContextEventStore;
use rehydration_domain::{ContextEventChange, ContextEventStore, ContextUpdatedEvent};
use std::sync::Arc;
use std::time::SystemTime;

fn sample_cmd_event(root: &str, role: &str, revision: u64) -> ContextUpdatedEvent {
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
        idempotency_key: None,
        logical_digest: None,
        requested_by: Some("test".to_string()),
        occurred_at: SystemTime::now(),
    }
}

#[tokio::test]
async fn valkey_event_store_append_and_read() -> Result<(), Box<dyn Error + Send + Sync>> {
    let container = start_valkey_container().await?;
    let port = container
        .get_host_port_ipv4(VALKEY_INTERNAL_PORT.tcp())
        .await?;
    let uri = format!("redis://127.0.0.1:{port}");

    let store = ValkeyContextEventStore::new(&uri)?;

    assert_eq!(store.current_revision("n1", "dev").await?, 0);

    let rev = store.append(sample_cmd_event("n1", "dev", 1), 0).await?;
    assert_eq!(rev, 1);
    assert_eq!(store.current_revision("n1", "dev").await?, 1);

    let rev2 = store.append(sample_cmd_event("n1", "dev", 2), 1).await?;
    assert_eq!(rev2, 2);

    Ok(())
}

#[tokio::test]
async fn valkey_event_store_rejects_wrong_revision() -> Result<(), Box<dyn Error + Send + Sync>> {
    let container = start_valkey_container().await?;
    let port = container
        .get_host_port_ipv4(VALKEY_INTERNAL_PORT.tcp())
        .await?;
    let uri = format!("redis://127.0.0.1:{port}");

    let store = ValkeyContextEventStore::new(&uri)?;

    let err = store
        .append(sample_cmd_event("n1", "dev", 1), 99)
        .await
        .expect_err("wrong revision should fail");

    assert!(
        matches!(err, rehydration_ports::PortError::Conflict(_)),
        "should be Conflict, got: {err}"
    );
    Ok(())
}

#[tokio::test]
async fn valkey_concurrent_appends_one_wins_one_conflicts()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let container = start_valkey_container().await?;
    let port = container
        .get_host_port_ipv4(VALKEY_INTERNAL_PORT.tcp())
        .await?;
    let uri = format!("redis://127.0.0.1:{port}");

    let store_a = Arc::new(ValkeyContextEventStore::new(&uri)?);
    let store_b = Arc::new(ValkeyContextEventStore::new(&uri)?);

    let sa = Arc::clone(&store_a);
    let sb = Arc::clone(&store_b);

    let task_a =
        tokio::spawn(async move { sa.append(sample_cmd_event("race", "dev", 1), 0).await });
    let task_b =
        tokio::spawn(async move { sb.append(sample_cmd_event("race", "dev", 1), 0).await });

    let (result_a, result_b) = tokio::join!(task_a, task_b);
    let result_a = result_a.expect("task a should not panic");
    let result_b = result_b.expect("task b should not panic");

    let (winner, loser) = match (&result_a, &result_b) {
        (Ok(_), Err(_)) => (result_a, result_b),
        (Err(_), Ok(_)) => (result_b, result_a),
        (Ok(_), Ok(_)) => panic!("both appends succeeded — CAS is broken"),
        (Err(a), Err(b)) => panic!("both failed: a={a}, b={b}"),
    };

    assert_eq!(winner.expect("winner"), 1);
    assert!(
        matches!(
            loser.expect_err("loser"),
            rehydration_ports::PortError::Conflict(_)
        ),
        "loser should get Conflict"
    );

    assert_eq!(store_a.current_revision("race", "dev").await?, 1);
    Ok(())
}
