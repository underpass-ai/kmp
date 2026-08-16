//! End-to-end smoke: a real embedded kernel in a temp dir, memory ingested
//! through the same facade the agent uses, and every viewer route exercised
//! over real HTTP on an ephemeral loopback port.

use std::sync::Arc;

use kmp_application::{
    MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData, MemoryEvidenceData,
    MemoryIngestCommand, MemoryRelationData,
};
use kmp_embedded::EmbeddedKernel;
use kmp_viewer::{MemoryViewerServer, bind_loopback};

const ABOUT: &str = "project:viewer-smoke";

fn entry(id: &str, text: &str, occurred_at: &str, sequence: u32) -> MemoryEntryData {
    MemoryEntryData {
        id: id.to_string(),
        kind: "decision".to_string(),
        text: text.to_string(),
        coordinates: vec![MemoryCoordinateData {
            dimension: "timeline".to_string(),
            scope_id: "timeline:work".to_string(),
            occurred_at: Some(occurred_at.to_string()),
            observed_at: None,
            ingested_at: None,
            valid_from: None,
            valid_until: None,
            sequence: Some(sequence),
            rank: None,
            metadata: Default::default(),
        }],
        metadata: Default::default(),
    }
}

fn corpus() -> MemoryIngestCommand {
    MemoryIngestCommand {
        about: ABOUT.to_string(),
        memory: MemoryData {
            dimensions: vec![MemoryDimensionData {
                id: "timeline:work".to_string(),
                kind: "timeline".to_string(),
                title: None,
                metadata: Default::default(),
            }],
            entries: vec![
                entry("decision:first", "Choose redb.", "2026-07-01T10:00:00Z", 1),
                entry(
                    "decision:second",
                    "Embed the viewer.",
                    "2026-07-02T10:00:00Z",
                    2,
                ),
            ],
            relations: vec![MemoryRelationData {
                source_ref: "decision:second".to_string(),
                target_ref: "decision:first".to_string(),
                rel: "follows".to_string(),
                semantic_class: "causal".to_string(),
                why: Some("the store choice made an in-process viewer possible".to_string()),
                evidence: Some("evidence:first".to_string()),
                confidence: Some("high".to_string()),
                sequence: Some(2),
                motivation: None,
                method: None,
                decision_id: None,
                caused_by_node_id: None,
                coordinate: None,
            }],
            evidence: vec![MemoryEvidenceData {
                id: "evidence:first".to_string(),
                supports: vec!["decision:first".to_string()],
                text: "Benchmarks and the ADR-009 measurements.".to_string(),
                source: None,
                time: None,
                metadata: Default::default(),
            }],
        },
        provenance: None,
        idempotency_key: "viewer-smoke-1".to_string(),
        dry_run: false,
    }
}

async fn get(port: u16, path_and_query: &str) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect to viewer");
    let request = format!(
        "GET {path_and_query} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("send request");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read response");
    let text = String::from_utf8_lossy(&raw);
    let status: u16 = text
        .split_whitespace()
        .nth(1)
        .expect("status code present")
        .parse()
        .expect("numeric status");
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
    let json = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test(flavor = "multi_thread")]
async fn every_viewer_route_serves_the_ingested_memory() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel opens");
    kernel
        .service()
        .ingest(corpus())
        .await
        .expect("memory ingests");

    let viewer = Arc::new(MemoryViewerServer::new(
        kernel.service(),
        Some(data_dir.path().display().to_string()),
    ));
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    tokio::spawn(viewer.serve(listener));

    let (status, info) = get(port, "/api/info").await;
    assert_eq!(status, 200);
    assert!(info["data_dir"].as_str().is_some());

    let (status, abouts) = get(port, "/api/abouts").await;
    assert_eq!(status, 200);
    let abouts_list = abouts["abouts"].as_array().expect("abouts array");
    assert!(
        abouts_list.iter().any(|a| a.as_str() == Some(ABOUT)),
        "ingested about is listed, got {abouts_list:?}"
    );

    let (status, graph) = get(port, &format!("/api/graph?about={}", urlencode(ABOUT))).await;
    assert_eq!(status, 200, "graph failed: {graph}");
    let nodes = graph["nodes"].as_array().expect("nodes");
    assert!(
        nodes.len() >= 3,
        "root + two decisions, got {}",
        nodes.len()
    );
    let edges = graph["edges"].as_array().expect("edges");
    assert!(
        edges.iter().any(|e| e["rel"] == "follows"
            && e["why"]
                .as_str()
                .is_some_and(|w| w.contains("in-process viewer"))),
        "typed relation with its why survives to the wire, got {edges}",
        edges = serde_json::to_string(&edges).expect("edges serialize")
    );
    assert!(
        graph["rendered"]["content"]
            .as_str()
            .is_some_and(|c| !c.is_empty())
    );
    assert!(graph["quality"]["compression_ratio"].as_f64().is_some());

    let (status, node) = get(
        port,
        &format!("/api/node?id={}&raw=1", urlencode("decision:first")),
    )
    .await;
    assert_eq!(status, 200, "node failed: {node}");
    assert_eq!(node["node"]["id"], "decision:first");
    assert!(
        node["incoming"].as_array().is_some_and(|r| !r.is_empty()),
        "decision:first has the `follows` edge incoming"
    );

    let (status, batch) = get(
        port,
        &format!(
            "/api/nodes?ids={}",
            urlencode("decision:first,decision:second,node:unknown")
        ),
    )
    .await;
    assert_eq!(status, 200, "nodes failed: {batch}");
    assert_eq!(batch["nodes"].as_array().expect("batch nodes").len(), 2);
    assert_eq!(batch["missing"][0], "node:unknown");

    let (status, timeline) = get(
        port,
        &format!(
            "/api/timeline?about={}&direction=goto&time={}",
            urlencode(ABOUT),
            urlencode("2026-07-03T00:00:00Z")
        ),
    )
    .await;
    assert_eq!(status, 200, "timeline failed: {timeline}");
    let entries = timeline["entries"].as_array().expect("entries");
    assert!(
        entries.iter().any(|e| e["ref_id"] == "decision:second"),
        "known-at-time read reaches the later decision, got {timeline}"
    );

    let (status, trace) = get(
        port,
        &format!(
            "/api/trace?from={}&to={}",
            urlencode("decision:second"),
            urlencode("decision:first")
        ),
    )
    .await;
    assert_eq!(status, 200, "trace failed: {trace}");
    assert!(trace["edges"].as_array().is_some_and(|e| !e.is_empty()));

    // The UI itself is served, and unknown paths and hosts are refused.
    let (status, _) = get(port, "/").await;
    assert_eq!(status, 200);
    let (status, _) = get(port, "/assets/pixi.min.js").await;
    assert_eq!(status, 200);
    let (status, _) = get(port, "/api/nope").await;
    assert_eq!(status, 404);
    let (status, error) = get(port, "/api/graph?about=about:missing").await;
    assert_eq!(status, 404, "unknown about maps to 404: {error}");
}

#[tokio::test]
async fn non_local_hosts_are_refused() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel opens");
    let viewer = Arc::new(MemoryViewerServer::new(kernel.service(), None));
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    tokio::spawn(viewer.serve(listener));

    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    stream
        .write_all(b"GET /api/info HTTP/1.1\r\nHost: evil.example\r\nConnection: close\r\n\r\n")
        .await
        .expect("send");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");
    let text = String::from_utf8_lossy(&raw);
    assert!(
        text.starts_with("HTTP/1.1 403"),
        "DNS-rebinding host is refused, got: {}",
        text.lines().next().unwrap_or_default()
    );
}

#[test]
fn loopback_binding_refuses_public_addresses() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let error = runtime
        .block_on(bind_loopback("0.0.0.0:0"))
        .expect_err("public bind must be refused");
    assert!(error.to_string().contains("loopback"));
}

/// The queries the UI issues have to be queries that answer.
///
/// Replay used to ask for `direction=goto` with a bare sequence, which
/// resolves no dimension and no scope: it came back empty on memory that had
/// entries, and the button reported the memory as empty. The timeline panel
/// defaulted to the same direction and rendered nothing at all. Neither
/// failure was visible from the server side, because a cursor that resolves
/// nothing is a 200 with an empty page — so this pins the shapes instead.
#[tokio::test(flavor = "multi_thread")]
async fn the_cursors_the_ui_issues_return_the_entries_they_should() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel opens");
    kernel
        .service()
        .ingest(corpus())
        .await
        .expect("memory ingests");

    let viewer = Arc::new(MemoryViewerServer::new(
        kernel.service(),
        Some(data_dir.path().display().to_string()),
    ));
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    tokio::spawn(viewer.serve(listener));

    // What Replay asks for: `near`, wide in both directions, anchored on a
    // time. It needs nothing selected and must return the whole line.
    let (status, replay) = get(
        port,
        &format!(
            "/api/timeline?about={}&direction=near&time={}&before=256&after=256",
            urlencode(ABOUT),
            urlencode("2026-07-03T00:00:00Z")
        ),
    )
    .await;
    assert_eq!(status, 200, "replay query failed: {replay}");
    let returned = replay["page"]["returned"].as_u64().expect("returned count");
    assert_eq!(
        returned,
        replay["page"]["total"].as_u64().expect("total count"),
        "a wide `near` window must return the whole line, got {replay}"
    );
    assert!(
        returned > 0,
        "replay would report an empty memory: {replay}"
    );

    // `goto` walks by temporal position. A ref hands it one directly, so it
    // answers whatever the entries look like.
    let (status, at_entry) = get(
        port,
        &format!(
            "/api/timeline?about={}&direction=goto&ref={}&before=8&after=8",
            urlencode(ABOUT),
            urlencode("decision:first")
        ),
    )
    .await;
    assert_eq!(status, 200, "goto-by-ref failed: {at_entry}");
    assert!(
        at_entry["page"]["returned"].as_u64().is_some_and(|n| n > 0),
        "`goto` with a ref must answer, got {at_entry}"
    );

    // The same direction with only a timestamp works *here* because this
    // corpus writes a `sequence` on every coordinate. `sequence` is optional
    // at ingest, and on memory written without it these directions answer
    // 0/0 — which is the case the UI has to explain rather than render blank.
    let (status, at_time) = get(
        port,
        &format!(
            "/api/timeline?about={}&direction=goto&time={}&before=8&after=8",
            urlencode(ABOUT),
            urlencode("2026-07-03T00:00:00Z")
        ),
    )
    .await;
    assert_eq!(status, 200, "goto-by-time failed: {at_time}");
    assert!(
        at_time["page"]["returned"].as_u64().is_some_and(|n| n > 0),
        "sequenced entries must be reachable by time as well: {at_time}"
    );
    assert!(
        at_time["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .all(|entry| entry["coordinates"]
                .as_array()
                .expect("coordinates")
                .iter()
                .any(|c| c["sequence"].is_number())),
        "the property that makes it work is the sequence on the coordinate: {at_time}"
    );
}

fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}
