use std::collections::BTreeMap;

use kmp_ports::{NodeDetailProjection, NodeProjection, PortError, ProjectionMutation};

use super::endpoint::{Neo4jEndpoint, parse_authority, parse_host_port, split_uri};
use super::projection_store::Neo4jProjectionStore;
use super::row_mapping::serialize_properties;

#[test]
fn endpoint_supports_auth_segments() {
    let endpoint = Neo4jEndpoint::parse("neo4j://neo4j:neo@localhost:7687".to_string())
        .expect("uri should parse");

    assert_eq!(endpoint.connection_uri, "neo4j://localhost:7687");
    assert_eq!(endpoint.user, "neo4j");
    assert_eq!(endpoint.password, "neo");
    assert!(endpoint.tls_ca_path.is_none());
}

#[test]
fn projection_store_keeps_endpoint_configuration() {
    let store =
        Neo4jProjectionStore::new("neo4j://neo4j:secret@localhost:7687").expect("store init");

    let rendered = format!("{store:?}");
    assert!(rendered.contains("Neo4jProjectionStore"));
    assert!(rendered.contains("connected: false"));
}

#[test]
fn endpoint_accepts_tls_ca_path_for_secure_schemes() {
    let endpoint = Neo4jEndpoint::parse(
        "bolt+s://neo4j:secret@localhost:7687?tls_ca_path=/tmp/neo4j-ca.pem".to_string(),
    )
    .expect("secure uri should accept a tls_ca_path");

    assert_eq!(endpoint.connection_uri, "bolt+s://localhost:7687");
    assert_eq!(endpoint.user, "neo4j");
    assert_eq!(endpoint.password, "secret");
    assert_eq!(
        endpoint.tls_ca_path.as_deref(),
        Some(std::path::Path::new("/tmp/neo4j-ca.pem"))
    );
}

#[test]
fn endpoint_rejects_unknown_query_params_and_paths() {
    let with_query = Neo4jEndpoint::parse("neo4j://localhost:7687?db=neo4j".to_string())
        .expect_err("unknown query params are not supported");
    let with_path = Neo4jEndpoint::parse("neo4j://localhost:7687/graph".to_string())
        .expect_err("paths are not supported");

    assert_eq!(
        with_query,
        PortError::InvalidState("unsupported graph uri option `db`".to_string())
    );
    assert_eq!(
        with_path,
        PortError::InvalidState("graph uri path segments are not supported".to_string())
    );
}

#[test]
fn endpoint_rejects_tls_ca_path_for_plaintext_schemes() {
    let error =
        Neo4jEndpoint::parse("neo4j://localhost:7687?tls_ca_path=/tmp/neo4j-ca.pem".to_string())
            .expect_err("plaintext uri should reject tls_ca_path");

    assert_eq!(
        error,
        PortError::InvalidState(
            "graph tls_ca_path requires bolt+s, bolt+ssc, neo4j+s, or neo4j+ssc".to_string()
        )
    );
}

#[test]
fn parser_rejects_invalid_scheme() {
    let error = Neo4jEndpoint::parse("https://localhost:7687".to_string())
        .expect_err("unsupported schemes must fail");

    assert_eq!(
        error,
        PortError::InvalidState("unsupported graph scheme `https`".to_string())
    );
}

#[test]
fn parser_rejects_missing_scheme_and_host() {
    let missing_scheme =
        Neo4jEndpoint::parse("localhost:7687".to_string()).expect_err("scheme is required");
    let missing_host = Neo4jEndpoint::parse("neo4j://".to_string()).expect_err("host is required");

    assert_eq!(
        missing_scheme,
        PortError::InvalidState("graph uri must include a scheme".to_string())
    );
    assert_eq!(
        missing_host,
        PortError::InvalidState("graph uri must include a host".to_string())
    );
}

#[test]
fn parser_rejects_unsupported_authorities() {
    let missing_password =
        parse_authority("neo4j@localhost:7687", "graph").expect_err("auth must include password");
    let invalid_separator =
        parse_host_port("[::1]7687", "graph").expect_err("ipv6 port separator must be explicit");
    let invalid_port =
        parse_host_port("localhost:not-a-port", "graph").expect_err("port must be numeric");

    assert_eq!(
        missing_password,
        PortError::InvalidState(
            "graph uri auth segments must include username and password".to_string()
        )
    );
    assert_eq!(
        invalid_separator,
        PortError::InvalidState("graph uri contains an invalid port separator".to_string())
    );
    assert!(
        invalid_port
            .to_string()
            .starts_with("graph uri contains an invalid port:")
    );
}

#[test]
fn split_uri_supports_ipv6_without_losing_authority() {
    let uri = split_uri("neo4j://[::1]:7687", "graph").expect("uri should parse");
    parse_host_port(uri.authority, "graph").expect("ipv6 authority should be valid");

    assert_eq!(uri.scheme, "neo4j");
    assert_eq!(uri.authority, "[::1]:7687");
    assert!(uri.query.is_none());
}

#[test]
fn serialize_properties_emits_json_object() {
    let properties = BTreeMap::from([
        ("phase".to_string(), "build".to_string()),
        ("role".to_string(), "developer".to_string()),
    ]);

    let serialized = serialize_properties(&properties).expect("properties should serialize");
    let parsed: serde_json::Value = serde_json::from_str(&serialized).expect("json should parse");

    assert_eq!(parsed["phase"], serde_json::json!("build"));
    assert_eq!(parsed["role"], serde_json::json!("developer"));
}

#[test]
fn node_detail_mutation_variant_is_reserved_for_valkey() {
    let mutation = ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
        node_id: "node-123".to_string(),
        detail: "expanded".to_string(),
        content_hash: "hash-123".to_string(),
        revision: 1,
    });

    match mutation {
        ProjectionMutation::UpsertNodeDetail(detail) => {
            assert_eq!(detail.node_id, "node-123");
        }
        other => panic!("unexpected mutation: {other:?}"),
    }
}

#[test]
fn node_projection_properties_can_be_prepared_for_persistence() {
    let projection = NodeProjection {
        node_id: "node-123".to_string(),
        node_kind: "capability".to_string(),
        title: "Projection foundation".to_string(),
        summary: "Node centric".to_string(),
        status: "ACTIVE".to_string(),
        labels: vec!["projection".to_string()],
        properties: BTreeMap::from([("phase".to_string(), "build".to_string())]),
        provenance: None,
    };

    let serialized =
        serialize_properties(&projection.properties).expect("properties should serialize");
    assert!(serialized.contains("\"phase\":\"build\""));
}
