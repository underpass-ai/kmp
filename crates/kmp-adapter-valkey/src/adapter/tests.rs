use std::str;
use std::time::{Duration, UNIX_EPOCH};

use kmp_domain::{
    BundleMetadata, BundleNode, BundleNodeDetail, CaseId, KmpBundle, Role, SnapshotSaveOptions,
};
use kmp_ports::{NodeDetailProjection, ProjectionMutation, ProjectionWriter};
use serde_json::{Value, json};

use super::endpoint::{parse_authority, parse_optional_port, split_uri};
use super::node_detail_serialization::serialize_node_detail;
use super::node_detail_store::ValkeyNodeDetailStore;
use super::processed_event_store::ValkeyProcessedEventStore;
use super::projection_checkpoint_serialization::{
    deserialize_projection_checkpoint, serialize_projection_checkpoint,
};
use super::projection_checkpoint_store::ValkeyProjectionCheckpointStore;
use super::resp::{RespValue, encode_set_command, map_valkey_response};
use super::snapshot_store::ValkeySnapshotStore;
use kmp_ports::ProjectionCheckpoint;

#[test]
fn snapshot_store_encodes_a_resp_set_command() {
    let store = ValkeySnapshotStore::new(
        "redis://localhost:6379?key_prefix=rehydration:test&ttl_seconds=60",
    )
    .expect("uri should be accepted");
    let case_id = CaseId::new("node-123").expect("case id is valid");
    let role = Role::new("reviewer").expect("role is valid");
    let bundle = sample_bundle(
        case_id.clone(),
        role.clone(),
        "bundle for node node-123 role reviewer",
        BundleMetadata::initial("0.1.0"),
    );

    let key = store.snapshot_key(&bundle);
    let payload = store
        .snapshot_payload(&bundle)
        .expect("snapshot payload should serialize");
    let request = parse_resp_array(&encode_set_command(
        &key,
        &payload,
        store.endpoint.ttl_seconds,
    ))
    .expect("resp command should be parsed");

    assert_eq!(request[0], "SET");
    assert_eq!(request[1], "rehydration:test:node-123:reviewer");
    assert_eq!(request[3], "EX");
    assert_eq!(request[4], "60");
    assert_eq!(
        serde_json::from_str::<Value>(&request[2]).expect("payload should be valid json"),
        json!({
            "root_node_id": "node-123",
            "role": "reviewer",
            "root_node": {
                "node_id": "node-123",
                "node_kind": "capability",
                "title": "Node node-123",
                "summary": "bundle for node node-123 role reviewer",
                "status": "ACTIVE",
                "labels": ["projection-node"],
                "properties": {}
            },
            "neighbor_nodes": [],
            "relationships": [],
            "node_details": [{
                "node_id": "node-123",
                "detail": "bundle for node node-123 role reviewer",
                "content_hash": "pending",
                "revision": 1
            }],
            "stats": {
                "selected_nodes": 1,
                "selected_relationships": 0,
                "detailed_nodes": 1
            },
            "metadata": {
                "revision": 1,
                "content_hash": "pending",
                "generator_version": "0.1.0"
            }
        })
    );
}

#[test]
fn snapshot_store_surfaces_server_errors() {
    let error = map_valkey_response(RespValue::Error("ERR read only replica".to_string()))
        .expect_err("server errors must surface");

    assert_eq!(
        error,
        kmp_ports::PortError::Unavailable(
            "valkey rejected write: ERR read only replica".to_string()
        )
    );
}

#[test]
fn snapshot_store_rejects_invalid_scheme() {
    let error = ValkeySnapshotStore::new("http://localhost:6379")
        .expect_err("unsupported schemes must fail");

    assert_eq!(
        error,
        kmp_ports::PortError::InvalidState("unsupported snapshot scheme `http`".to_string())
    );
}

#[test]
fn snapshot_store_parses_valid_runtime_options() {
    let store = ValkeySnapshotStore::new(
        "valkey://cache.internal?key_prefix=rehydration:it&ttl_seconds=15",
    )
    .expect("valkey uri should be accepted");

    assert_eq!(store.endpoint.host, "cache.internal");
    assert_eq!(store.endpoint.port, 6379);
    assert_eq!(store.endpoint.key_prefix, "rehydration:it");
    assert_eq!(store.endpoint.ttl_seconds, Some(15));
    assert!(!store.endpoint.tls.enabled);
}

#[test]
fn snapshot_store_parses_secure_runtime_options() {
    let store = ValkeySnapshotStore::new(
        "rediss://cache.internal:6380?key_prefix=rehydration:tls&tls_ca_path=/tmp/ca.pem&tls_cert_path=/tmp/client.pem&tls_key_path=/tmp/client.key",
    )
    .expect("rediss uri should be accepted");

    assert_eq!(store.endpoint.host, "cache.internal");
    assert_eq!(store.endpoint.port, 6380);
    assert_eq!(store.endpoint.key_prefix, "rehydration:tls");
    assert!(store.endpoint.tls.enabled);
    assert_eq!(
        store.endpoint.tls.ca_path.as_deref(),
        Some(std::path::Path::new("/tmp/ca.pem"))
    );
    assert_eq!(
        store.endpoint.tls.cert_path.as_deref(),
        Some(std::path::Path::new("/tmp/client.pem"))
    );
    assert_eq!(
        store.endpoint.tls.key_path.as_deref(),
        Some(std::path::Path::new("/tmp/client.key"))
    );
}

#[test]
fn snapshot_store_prefers_request_ttl_over_endpoint_default() {
    let store = ValkeySnapshotStore::new(
        "redis://localhost:6379?key_prefix=rehydration:test&ttl_seconds=60",
    )
    .expect("uri should be accepted");
    let case_id = CaseId::new("node-123").expect("case id is valid");
    let role = Role::new("reviewer").expect("role is valid");
    let bundle = sample_bundle(
        case_id,
        role,
        "bundle for node node-123 role reviewer",
        BundleMetadata::initial("0.1.0"),
    );

    let key = store.snapshot_key(&bundle);
    let payload = store
        .snapshot_payload(&bundle)
        .expect("snapshot payload should serialize");
    let ttl_seconds = store.effective_ttl_seconds(SnapshotSaveOptions::new(Some(15)));
    let request = parse_resp_array(&encode_set_command(&key, &payload, ttl_seconds))
        .expect("resp command should be parsed");

    assert_eq!(request[4], "15");
}

#[test]
fn node_detail_store_uses_dedicated_default_prefix() {
    let store = ValkeyNodeDetailStore::new("redis://cache.internal")
        .expect("detail uri should be accepted");

    assert_eq!(store.endpoint.key_prefix, "rehydration:node-detail");
    assert_eq!(
        store.detail_key("node-123"),
        "rehydration:node-detail:node-123"
    );
}

#[test]
fn processed_event_store_uses_dedicated_default_prefix() {
    let store = ValkeyProcessedEventStore::new("redis://cache.internal")
        .expect("processed event uri should be accepted");

    assert_eq!(
        store.processed_event_key("projection-consumer", "event-123"),
        "rehydration:processed-event:projection-consumer:event-123"
    );
}

#[test]
fn processed_event_store_rejects_invalid_runtime_state_uri() {
    let error = ValkeyProcessedEventStore::new("http://cache.internal")
        .expect_err("unsupported schemes must fail");

    assert_eq!(
        error,
        kmp_ports::PortError::InvalidState("unsupported processed event scheme `http`".to_string())
    );
}

#[test]
fn projection_checkpoint_store_uses_dedicated_default_prefix() {
    let store = ValkeyProjectionCheckpointStore::new("redis://cache.internal")
        .expect("projection checkpoint uri should be accepted");

    assert_eq!(
        store.checkpoint_key("projection-consumer", "graph.node.materialized"),
        "rehydration:projection-checkpoint:projection-consumer:graph.node.materialized"
    );
}

#[test]
fn projection_checkpoint_store_rejects_invalid_runtime_state_uri() {
    let error = ValkeyProjectionCheckpointStore::new("http://cache.internal")
        .expect_err("unsupported schemes must fail");

    assert_eq!(
        error,
        kmp_ports::PortError::InvalidState(
            "unsupported projection checkpoint scheme `http`".to_string()
        )
    );
}

#[test]
fn projection_checkpoint_serialization_roundtrip_is_stable() {
    let checkpoint = ProjectionCheckpoint {
        consumer_name: "projection-consumer".to_string(),
        stream_name: "graph.node.materialized".to_string(),
        last_subject: "rehydration.graph.node.materialized".to_string(),
        last_event_id: "event-123".to_string(),
        last_correlation_id: "corr-123".to_string(),
        last_occurred_at: "2026-03-12T00:00:00Z".to_string(),
        processed_events: 4,
        updated_at: UNIX_EPOCH + Duration::from_millis(1234),
    };

    let payload =
        serialize_projection_checkpoint(&checkpoint).expect("checkpoint should serialize");
    let decoded = deserialize_projection_checkpoint(&payload)
        .expect("checkpoint should deserialize")
        .expect("checkpoint should be present");

    assert_eq!(decoded, checkpoint);
}

#[test]
fn snapshot_store_rejects_invalid_query_options() {
    let invalid_pair = ValkeySnapshotStore::new("redis://localhost:6379?ttl_seconds")
        .expect_err("query pairs must contain =");
    let invalid_ttl = ValkeySnapshotStore::new("redis://localhost:6379?ttl_seconds=soon")
        .expect_err("ttl must be numeric");
    let empty_prefix = ValkeySnapshotStore::new("redis://localhost:6379?key_prefix=   ")
        .expect_err("key prefix cannot be empty");
    let unsupported_option = ValkeySnapshotStore::new("redis://localhost:6379?database=1")
        .expect_err("unsupported options must fail");
    let plaintext_tls = ValkeySnapshotStore::new("redis://localhost:6379?tls_ca_path=/tmp/ca.pem")
        .expect_err("TLS options should require a secure scheme");
    let partial_identity =
        ValkeySnapshotStore::new("rediss://localhost:6379?tls_cert_path=/tmp/client.pem")
            .expect_err("client identity should require a key pair");

    assert_eq!(
        invalid_pair,
        kmp_ports::PortError::InvalidState(
            "snapshot uri query parameter `ttl_seconds` is invalid".to_string()
        )
    );
    assert!(
        invalid_ttl
            .to_string()
            .starts_with("snapshot ttl_seconds must be an integer:")
    );
    assert_eq!(
        empty_prefix,
        kmp_ports::PortError::InvalidState("snapshot key_prefix cannot be empty".to_string())
    );
    assert_eq!(
        unsupported_option,
        kmp_ports::PortError::InvalidState(
            "unsupported snapshot uri option `database`".to_string()
        )
    );
    assert_eq!(
        plaintext_tls,
        kmp_ports::PortError::InvalidState(
            "snapshot TLS options require rediss:// or valkeys://".to_string()
        )
    );
    assert_eq!(
        partial_identity,
        kmp_ports::PortError::InvalidState(
            "snapshot tls_cert_path and tls_key_path must be configured together".to_string()
        )
    );
}

#[test]
fn parser_helpers_cover_ipv6_and_error_branches() {
    let (scheme, authority, query) =
        split_uri("redis://[::1]:6380/cache?ttl_seconds=5", "snapshot").expect("uri should parse");
    let (host, port) =
        parse_authority(authority, 6379, "snapshot").expect("authority should parse");

    assert_eq!(scheme, "redis");
    assert_eq!(authority, "[::1]:6380");
    assert_eq!(query, Some("ttl_seconds=5"));
    assert_eq!(host, "[::1]");
    assert_eq!(port, 6380);

    let missing_scheme = split_uri("localhost:6379", "snapshot").expect_err("scheme is required");
    let missing_host = split_uri("redis://", "snapshot").expect_err("host is required");
    let auth_segment = parse_authority("user@localhost:6379", 6379, "snapshot")
        .expect_err("auth is not supported");
    let invalid_ipv6 =
        parse_authority("[::1", 6379, "snapshot").expect_err("ipv6 host must be complete");
    let invalid_port = parse_authority("localhost:not-a-port", 6379, "snapshot")
        .expect_err("ports must be numeric");
    let invalid_separator = parse_optional_port("6380", 6379, "snapshot")
        .expect_err("port separators must be explicit");

    assert_eq!(
        missing_scheme,
        kmp_ports::PortError::InvalidState("snapshot uri must include a scheme".to_string())
    );
    assert_eq!(
        missing_host,
        kmp_ports::PortError::InvalidState("snapshot uri must include a host".to_string())
    );
    assert_eq!(
        auth_segment,
        kmp_ports::PortError::InvalidState(
            "snapshot uri auth segments are not supported yet".to_string()
        )
    );
    assert_eq!(
        invalid_ipv6,
        kmp_ports::PortError::InvalidState(
            "snapshot uri contains an invalid IPv6 host".to_string()
        )
    );
    assert!(
        invalid_port
            .to_string()
            .starts_with("snapshot uri contains an invalid port:")
    );
    assert_eq!(
        invalid_separator,
        kmp_ports::PortError::InvalidState(
            "snapshot uri contains an invalid port separator".to_string()
        )
    );
}

#[test]
fn serializer_and_response_helpers_cover_escape_paths() {
    let case_id = CaseId::new("node-123").expect("case id is valid");
    let role = Role::new("reviewer").expect("role is valid");
    let bundle = sample_bundle(
        case_id.clone(),
        role.clone(),
        "quote \" slash \\ newline\n tab\t",
        BundleMetadata {
            revision: 2,
            content_hash: "hash\rvalue".to_string(),
            generator_version: "0.1.0".to_string(),
        },
    );
    let store = ValkeySnapshotStore::new("redis://localhost").expect("uri should be accepted");
    let payload = store
        .snapshot_payload(&bundle)
        .expect("snapshot payload should serialize");
    let payload_json =
        serde_json::from_str::<Value>(&payload).expect("payload should be valid json");

    assert!(payload.contains("\\\""));
    assert!(payload.contains("\\\\"));
    assert!(payload.contains("\\n"));
    assert!(payload.contains("\\t"));
    assert!(payload.contains("\\r"));
    assert_eq!(
        payload_json["root_node"]["summary"],
        Value::String("quote \" slash \\ newline\n tab\t".to_string())
    );
    assert!(matches!(
        map_valkey_response(RespValue::SimpleString("OK".to_string())),
        Ok(())
    ));
    assert_eq!(
        map_valkey_response(RespValue::BulkString(Some("?wat".to_string())))
            .expect_err("unexpected response must fail"),
        kmp_ports::PortError::Unavailable(
            "unexpected valkey response: BulkString(Some(\"?wat\"))".to_string()
        )
    );
}

#[test]
fn node_detail_serializer_emits_expected_shape() {
    let payload = serialize_node_detail(&NodeDetailProjection {
        node_id: "node-123".to_string(),
        detail: "Expanded detail".to_string(),
        content_hash: "hash-123".to_string(),
        revision: 4,
    })
    .expect("detail should serialize");

    assert_eq!(
        payload,
        "{\"content_hash\":\"hash-123\",\"detail\":\"Expanded detail\",\"node_id\":\"node-123\",\"revision\":4}"
    );
}

#[tokio::test]
async fn detail_store_rejects_graph_mutations() {
    let store =
        ValkeyNodeDetailStore::new("redis://cache.internal").expect("detail uri should parse");

    let error = store
        .apply_mutations(vec![ProjectionMutation::UpsertNode(
            kmp_ports::NodeProjection {
                node_id: "node-123".to_string(),
                node_kind: "capability".to_string(),
                title: "Projection".to_string(),
                summary: String::new(),
                status: "ACTIVE".to_string(),
                labels: Vec::new(),
                properties: std::collections::BTreeMap::new(),
                provenance: None,
            },
        )])
        .await
        .expect_err("graph nodes should be rejected by detail store");

    assert_eq!(
        error,
        kmp_ports::PortError::InvalidState(
            "valkey detail store does not persist graph node `node-123`".to_string()
        )
    );
}

fn sample_bundle(
    case_id: CaseId,
    role: Role,
    summary: &str,
    metadata: BundleMetadata,
) -> KmpBundle {
    KmpBundle::new(
        case_id.clone(),
        role,
        BundleNode::new(
            case_id.as_str(),
            "capability",
            format!("Node {}", case_id.as_str()),
            summary,
            "ACTIVE",
            vec!["projection-node".to_string()],
            std::collections::BTreeMap::new(),
        ),
        Vec::new(),
        Vec::new(),
        vec![BundleNodeDetail::new(
            case_id.as_str(),
            summary,
            metadata.content_hash.clone(),
            metadata.revision,
        )],
        metadata,
    )
    .expect("bundle should be valid")
}

fn parse_resp_array(
    buffer: &[u8],
) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let payload = str::from_utf8(buffer)?;
    let mut segments = payload.split("\r\n");
    let header = segments.next().ok_or("resp array header is missing")?;
    let argument_count = header
        .strip_prefix('*')
        .ok_or("resp array header is missing")?
        .parse::<usize>()?;

    let mut arguments = Vec::with_capacity(argument_count);
    for _ in 0..argument_count {
        let length_line = segments
            .next()
            .ok_or("resp bulk string header is missing")?;
        let length = length_line
            .strip_prefix('$')
            .ok_or("resp bulk string header is missing")?
            .parse::<usize>()?;
        let argument = segments
            .next()
            .ok_or("resp bulk string payload is missing")?;
        if argument.len() != length {
            return Err("resp bulk string payload has an unexpected length".into());
        }
        arguments.push(argument.to_string());
    }

    Ok(arguments)
}
