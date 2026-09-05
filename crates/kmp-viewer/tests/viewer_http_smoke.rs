//! End-to-end smoke: a real embedded kernel in a temp dir, memory ingested
//! through the same facade the agent uses, and every viewer route exercised
//! over real HTTP on an ephemeral loopback port.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use kmp_application::{
    MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData, MemoryEvidenceData,
    MemoryIngestCommand, MemoryRelationData,
};
use kmp_embedded::EmbeddedKernel;
use kmp_viewer::{MemoryViewerServer, bind_loopback};

const ABOUT: &str = "project:viewer-smoke";

static AUTH_COOKIES: OnceLock<Mutex<BTreeMap<u16, String>>> = OnceLock::new();

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
                entry(
                    "project:viewer-smoke:decision:first",
                    "Choose local storage.",
                    "2026-07-01T10:00:00Z",
                    1,
                ),
                entry(
                    "project:viewer-smoke:decision:second",
                    "Embed the viewer.",
                    "2026-07-02T10:00:00Z",
                    2,
                ),
            ],
            relations: vec![MemoryRelationData {
                source_ref: "project:viewer-smoke:decision:second".to_string(),
                target_ref: "project:viewer-smoke:decision:first".to_string(),
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
                id: "evidence:project:viewer-smoke:first".to_string(),
                supports: vec!["project:viewer-smoke:decision:first".to_string()],
                text: "Benchmarks and the ADR-009 measurements.".to_string(),
                source: None,
                time: None,
                metadata: Default::default(),
            }],
        },
        provenance: None,
        idempotency_key: "viewer-smoke-1".to_string(),
        dry_run: false,
        label_policy: Default::default(),
    }
}

async fn authorize(port: u16, invitation: &str) {
    let origin = format!("http://127.0.0.1:{port}");
    let path = invitation
        .strip_prefix(&origin)
        .expect("invitation belongs to this listener");
    let raw = raw_request(
        port,
        &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"),
    )
    .await;
    assert!(
        raw.starts_with("HTTP/1.1 303"),
        "bootstrap redirects: {raw}"
    );
    let cookie = raw
        .lines()
        .find_map(|line| line.strip_prefix("Set-Cookie: "))
        .and_then(|value| value.split(';').next())
        .expect("bootstrap sets a cookie")
        .to_string();
    AUTH_COOKIES
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .expect("test auth registry")
        .insert(port, cookie);
}

fn auth_cookie(port: u16) -> String {
    AUTH_COOKIES
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .expect("test auth registry")
        .get(&port)
        .cloned()
        .expect("viewer was bootstrapped")
}

async fn get(port: u16, path_and_query: &str) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect to viewer");
    let request = format!(
        "GET {path_and_query} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: {}\r\nConnection: close\r\n\r\n",
        auth_cookie(port)
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

    let viewer = Arc::new(
        MemoryViewerServer::new(
            kernel.service(),
            Some(data_dir.path().display().to_string()),
        )
        .expect("viewer creates capability"),
    );
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    let invitation = viewer.capability_url(&format!("http://127.0.0.1:{port}/"));
    tokio::spawn(viewer.serve(listener));
    authorize(port, &invitation).await;

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
        &format!(
            "/api/node?about={}&id={}&raw=1",
            urlencode(ABOUT),
            urlencode("project:viewer-smoke:decision:first")
        ),
    )
    .await;
    assert_eq!(status, 200, "node failed: {node}");
    assert_eq!(node["node"]["id"], "project:viewer-smoke:decision:first");
    assert!(
        node["incoming"].as_array().is_some_and(|r| !r.is_empty()),
        "decision:first has the `follows` edge incoming"
    );

    let (status, batch) = get(
        port,
        &format!(
            "/api/nodes?about={}&ids={}",
            urlencode(ABOUT),
            urlencode(
                "project:viewer-smoke:decision:first,project:viewer-smoke:decision:second,project:viewer-smoke:node:unknown"
            )
        ),
    )
    .await;
    assert_eq!(status, 200, "nodes failed: {batch}");
    assert_eq!(batch["nodes"].as_array().expect("batch nodes").len(), 2);
    assert_eq!(batch["missing"][0], "project:viewer-smoke:node:unknown");

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
        entries
            .iter()
            .any(|e| e["ref_id"] == "project:viewer-smoke:decision:second"),
        "known-at-time read reaches the later decision, got {timeline}"
    );

    let (status, projection) = get(
        port,
        &format!(
            "/api/projection?about={}&axis=occurred&from={}&to={}&lod=moment&bins=8&limit=8",
            urlencode(ABOUT),
            urlencode("2026-07-01T00:00:00Z"),
            urlencode("2026-07-03T00:00:00Z")
        ),
    )
    .await;
    assert_eq!(status, 200, "visual projection failed: {projection}");
    assert_eq!(projection["contract"], "kmp.visual.projection.v1");
    assert_eq!(projection["entries"].as_array().map(Vec::len), Some(2));
    assert!(
        projection["bins"]
            .as_array()
            .is_some_and(|bins| !bins.is_empty())
    );

    let (status, unavailable) = get(
        port,
        "/api/observability?from_ms=0&to_ms=1&series=causal_density",
    )
    .await;
    assert_eq!(
        status, 503,
        "missing adapter must be explicit: {unavailable}"
    );

    let (status, trace) = get(
        port,
        &format!(
            "/api/trace?about={}&from={}&to={}",
            urlencode(ABOUT),
            urlencode("project:viewer-smoke:decision:second"),
            urlencode("project:viewer-smoke:decision:first")
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
    let (status, _) = get(port, "/assets/loom-core.js").await;
    assert_eq!(status, 200, "the pure-logic asset is served");
    let (status, _) = get(port, "/assets/loom.js").await;
    assert_eq!(status, 200, "the loom app is served");
    let (status, _) = get(port, "/api/nope").await;
    assert_eq!(status, 404);

    // The view aggregate: a camera position an agent and a person share.
    // It answers before anyone opens it only with a refusal, never with a
    // made-up view.
    let (status, _) = get(port, "/api/view?id=nobody-opened-this").await;
    assert_eq!(status, 404, "a view nobody opened is not invented");
    let (status, error) = get(port, "/api/graph?about=about:missing").await;
    assert_eq!(status, 404, "unknown about maps to 404: {error}");
}

/// The transport boundary persists protobuf timestamps in a lexicographically
/// sortable `unix:` representation, while browsers send RFC3339 ranges. Both
/// spellings describe the same clock and therefore have to compare on one
/// temporal axis. A raw string comparison made every persisted entry sort
/// after an RFC3339 `to`, so `/api/projection` answered 0/0 for real stores.
#[tokio::test(flavor = "multi_thread")]
async fn projection_compares_rfc3339_ranges_with_persisted_sortable_clocks() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel opens");
    let mut stored = corpus();
    stored.memory.entries[0].coordinates[0].occurred_at =
        Some("unix:101782900000:000000000".to_string());
    stored.memory.entries[1].coordinates[0].occurred_at =
        Some("unix:101782986400:000000000".to_string());
    kernel
        .service()
        .ingest(stored)
        .await
        .expect("memory ingests");

    let viewer = Arc::new(
        MemoryViewerServer::new(
            kernel.service(),
            Some(data_dir.path().display().to_string()),
        )
        .expect("viewer creates capability"),
    );
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    let invitation = viewer.capability_url(&format!("http://127.0.0.1:{port}/"));
    tokio::spawn(viewer.serve(listener));
    authorize(port, &invitation).await;

    let (status, atlas) = get(port, &format!("/api/projection?about={}", urlencode(ABOUT))).await;
    assert_eq!(status, 200, "default visual projection failed: {atlas}");
    assert!(
        atlas["bins"]
            .as_array()
            .is_some_and(|bins| !bins.is_empty()),
        "the default atlas must contain the persisted clock: {atlas}"
    );
    assert_eq!(atlas["included_dimensions"][0], "timeline");
    assert_eq!(atlas["page"]["total"], 2);

    let (status, projection) = get(
        port,
        &format!(
            "/api/projection?about={}&axis=occurred&from={}&to={}&lod=moment&bins=8&limit=8",
            urlencode(ABOUT),
            urlencode("2026-07-01T00:00:00Z"),
            urlencode("2026-07-03T00:00:00Z")
        ),
    )
    .await;

    assert_eq!(status, 200, "visual projection failed: {projection}");
    assert_eq!(projection["entries"].as_array().map(Vec::len), Some(2));
    assert_eq!(projection["included_dimensions"][0], "timeline");
    assert_eq!(projection["page"]["total"], 2);
}

/// ChronoLoom asks for the wide `episode` extent first and derives the moment
/// window it then draws from that answer's cluster endpoints. When those
/// endpoints were serialized at whole-second precision the derived window
/// closed before the entry it described: a populated about rendered `0/0`
/// while its own episode projection reported one entry (#454).
#[tokio::test(flavor = "multi_thread")]
async fn a_sub_second_entry_survives_the_extent_to_moment_round_trip() {
    const OCCURRED_ABOUT: &str = "project:subsecond-occurred";
    const OBSERVED_ABOUT: &str = "project:subsecond-observed";
    // The spelling a real store hands back: sortable, and carrying the
    // nanoseconds the reported entries differed by.
    const OCCURRED_AT: &str = "unix:101788145953:731471000";
    const OBSERVED_AT: &str = "unix:101788145953:731475000";
    const OCCURRED_READABLE: &str = "2026-08-31T03:12:33.731471Z";
    const OBSERVED_READABLE: &str = "2026-08-31T03:12:33.731475Z";

    let data_dir = tempfile::tempdir().expect("temp data dir");
    let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel opens");
    // About A carries both clocks; about B carries only the observed one, so
    // its occurred axis stays legitimately empty (#421).
    kernel
        .service()
        .ingest(sub_second_corpus(
            OCCURRED_ABOUT,
            Some(OCCURRED_AT),
            OCCURRED_AT,
        ))
        .await
        .expect("about A ingests");
    kernel
        .service()
        .ingest(sub_second_corpus(OBSERVED_ABOUT, None, OBSERVED_AT))
        .await
        .expect("about B ingests");

    let viewer = Arc::new(
        MemoryViewerServer::new(
            kernel.service(),
            Some(data_dir.path().display().to_string()),
        )
        .expect("viewer creates capability"),
    );
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    let invitation = viewer.capability_url(&format!("http://127.0.0.1:{port}/"));
    tokio::spawn(viewer.serve(listener));
    authorize(port, &invitation).await;

    for (about, axis, expected) in [
        (OCCURRED_ABOUT, "occurred", OCCURRED_READABLE),
        (OBSERVED_ABOUT, "observed", OBSERVED_READABLE),
    ] {
        let extent = episode_extent(port, about, axis).await;
        assert_eq!(
            extent["page"]["total"], 1,
            "{about}/{axis} episode: {extent}"
        );

        let cluster = &extent["clusters"][0];
        let (from, to) = (
            cluster["from"].as_str().expect("cluster from"),
            cluster["to"].as_str().expect("cluster to"),
        );
        assert_eq!(from, expected, "{about}/{axis} keeps its fraction");
        assert_eq!(to, expected, "{about}/{axis} keeps its fraction");

        let (status, moment) = get(
            port,
            &format!(
                "/api/projection?about={}&axis={axis}&from={}&to={}&lod=moment&bins=8&limit=8",
                urlencode(about),
                urlencode(from),
                urlencode(&one_millisecond_past(to))
            ),
        )
        .await;
        assert_eq!(status, 200, "{about}/{axis} moment failed: {moment}");
        assert_eq!(
            moment["page"]["total"], 1,
            "a non-empty extent must produce a window that holds its entry: {moment}"
        );
        assert_eq!(moment["entries"].as_array().map(Vec::len), Some(1));
    }

    // The clock that carries nothing keeps saying so, plainly.
    let empty = episode_extent(port, OBSERVED_ABOUT, "occurred").await;
    assert_eq!(empty["page"]["total"], 0, "empty occurred clock: {empty}");
    assert!(
        empty["clusters"].as_array().is_some_and(Vec::is_empty),
        "an empty clock projects no cluster: {empty}"
    );
}

fn sub_second_corpus(
    about: &str,
    occurred_at: Option<&str>,
    observed_at: &str,
) -> MemoryIngestCommand {
    MemoryIngestCommand {
        about: about.to_string(),
        memory: MemoryData {
            dimensions: vec![MemoryDimensionData {
                id: "timeline:work".to_string(),
                kind: "timeline".to_string(),
                title: None,
                metadata: Default::default(),
            }],
            entries: vec![MemoryEntryData {
                id: format!("{about}:decision:only"),
                kind: "decision".to_string(),
                text: "One entry, written inside a single second.".to_string(),
                coordinates: vec![MemoryCoordinateData {
                    dimension: "timeline".to_string(),
                    scope_id: "timeline:work".to_string(),
                    occurred_at: occurred_at.map(str::to_string),
                    observed_at: Some(observed_at.to_string()),
                    ingested_at: None,
                    valid_from: None,
                    valid_until: None,
                    sequence: Some(1),
                    rank: None,
                    metadata: Default::default(),
                }],
                metadata: Default::default(),
            }],
            relations: Vec::new(),
            evidence: Vec::new(),
        },
        provenance: None,
        idempotency_key: format!("{about}-1"),
        dry_run: false,
        label_policy: Default::default(),
    }
}

/// The wide probe ChronoLoom issues before it knows where memory lives.
async fn episode_extent(port: u16, about: &str, axis: &str) -> serde_json::Value {
    let (status, projection) = get(
        port,
        &format!(
            "/api/projection?about={}&axis={axis}&from={}&to={}&lod=episode&bins=128&limit=2048",
            urlencode(about),
            urlencode("1900-01-01T00:00:00Z"),
            urlencode("2100-01-01T00:00:00Z")
        ),
    )
    .await;
    assert_eq!(status, 200, "episode probe failed: {projection}");
    projection
}

/// The browser's rule, in the one line this test needs: a moment window ends
/// one millisecond past the extent's last endpoint, because the projection
/// range is half-open and `Date.parse` floors anything below the millisecond.
/// The rule itself lives in `KMP_LOOM.projectionExtent`, with its own tests in
/// `crates/kmp-viewer/ui/loom-core.test.js`.
fn one_millisecond_past(instant: &str) -> String {
    let (head, fraction) = instant
        .trim_end_matches('Z')
        .split_once('.')
        .expect("the endpoint carries a fraction");
    let nanos: u32 = format!("{fraction:0<9}")
        .parse()
        .expect("a nanosecond fraction");
    let widened = nanos + 1_000_000;
    assert!(
        widened < 1_000_000_000,
        "this fixture never carries into the next second"
    );
    format!("{head}.{widened:09}Z")
}

#[tokio::test(flavor = "multi_thread")]
async fn coarse_kind_totals_count_entries_once_across_multiple_lanes() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel opens");
    let mut stored = corpus();
    stored.memory.dimensions.push(MemoryDimensionData {
        id: "task:work".to_string(),
        kind: "task".to_string(),
        title: None,
        metadata: Default::default(),
    });
    for entry in &mut stored.memory.entries {
        let mut coordinate = entry.coordinates[0].clone();
        coordinate.dimension = "task".to_string();
        coordinate.scope_id = "task:work".to_string();
        entry.coordinates.push(coordinate);
    }
    kernel
        .service()
        .ingest(stored)
        .await
        .expect("memory ingests");

    let viewer = Arc::new(
        MemoryViewerServer::new(
            kernel.service(),
            Some(data_dir.path().display().to_string()),
        )
        .expect("viewer creates capability"),
    );
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    let invitation = viewer.capability_url(&format!("http://127.0.0.1:{port}/"));
    tokio::spawn(viewer.serve(listener));
    authorize(port, &invitation).await;

    let (status, episode) = get(
        port,
        &format!(
            "/api/projection?about={}&axis=occurred&from={}&to={}&lod=episode&bins=8&limit=8",
            urlencode(ABOUT),
            urlencode("2026-07-01T00:00:00Z"),
            urlencode("2026-07-03T00:00:00Z")
        ),
    )
    .await;

    assert_eq!(status, 200, "visual projection failed: {episode}");
    assert_eq!(episode["page"]["total"], 2);
    assert_eq!(episode["by_kind"]["decision"], 2);
    let lane_memberships: u64 = episode["clusters"]
        .as_array()
        .expect("clusters")
        .iter()
        .map(|cluster| cluster["by_kind"]["decision"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(lane_memberships, 4, "fixture must exercise lane inflation");
}

#[tokio::test]
async fn non_local_hosts_are_refused() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel opens");
    let viewer = Arc::new(
        MemoryViewerServer::new(kernel.service(), None).expect("viewer creates capability"),
    );
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

#[tokio::test]
async fn a_session_capability_guards_memory_and_shared_view_state() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel opens");
    kernel
        .service()
        .ingest(corpus())
        .await
        .expect("memory ingests");
    let viewer = Arc::new(
        MemoryViewerServer::new(kernel.service(), None).expect("viewer creates capability"),
    );
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    let invitation = viewer.capability_url(&format!("http://127.0.0.1:{port}/"));
    tokio::spawn(viewer.serve(listener));

    let unauthenticated_read = raw_request(
        port,
        &format!("GET /api/abouts HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"),
    )
    .await;
    assert!(
        unauthenticated_read.starts_with("HTTP/1.1 401"),
        "a local socket alone cannot read memory: {unauthenticated_read}"
    );
    assert!(
        unauthenticated_read.contains("ask the agent to open the loom"),
        "the refusal must name a path to the capability: {unauthenticated_read}"
    );

    let view_id = format!("unauthorized-{port}");
    let unauthenticated_move = raw_request(
        port,
        &format!(
            "POST /api/view/open?id={view_id}&about={ABOUT} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert!(
        unauthenticated_move.starts_with("HTTP/1.1 401"),
        "a local socket alone cannot move the view: {unauthenticated_move}"
    );

    let wrong_capability = raw_request(
        port,
        &format!(
            "GET /?k=not-the-capability HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert!(wrong_capability.starts_with("HTTP/1.1 401"));

    let origin = format!("http://127.0.0.1:{port}");
    let bootstrap_path = invitation
        .strip_prefix(&origin)
        .expect("invitation belongs to listener");
    let bootstrap = raw_request(
        port,
        &format!(
            "GET {bootstrap_path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert!(bootstrap.starts_with("HTTP/1.1 303"), "{bootstrap}");
    assert!(bootstrap.contains("Location: /\r\n"));
    let set_cookie = bootstrap
        .lines()
        .find_map(|line| line.strip_prefix("Set-Cookie: "))
        .expect("bootstrap sets the capability cookie");
    assert!(set_cookie.contains("; HttpOnly; SameSite=Strict; Path=/"));
    let cookie = set_cookie
        .split(';')
        .next()
        .expect("cookie pair is present");

    let authorized_read = raw_request(
        port,
        &format!(
            "GET /api/abouts HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: {cookie}\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert!(
        authorized_read.starts_with("HTTP/1.1 200"),
        "the browser cookie opens the memory: {authorized_read}"
    );

    let untouched_view = raw_request(
        port,
        &format!(
            "GET /api/view?id={view_id} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: {cookie}\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert!(
        untouched_view.starts_with("HTTP/1.1 404"),
        "the refused POST created no view: {untouched_view}"
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

#[tokio::test]
async fn observability_unavailability_reports_its_actual_reason() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel opens");
    let viewer = Arc::new(
        MemoryViewerServer::new(kernel.service(), None)
            .expect("viewer creates capability")
            .with_observability_unavailable(
                "the store's quality telemetry is held by another process",
            ),
    );
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    let invitation = viewer.capability_url(&format!("http://127.0.0.1:{port}/"));
    tokio::spawn(viewer.serve(listener));
    authorize(port, &invitation).await;

    let (status, body) = get(port, "/api/observability?about=project:test").await;
    assert_eq!(status, 503);
    assert_eq!(
        body["error"],
        "the store's quality telemetry is held by another process"
    );
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

    let viewer = Arc::new(
        MemoryViewerServer::new(
            kernel.service(),
            Some(data_dir.path().display().to_string()),
        )
        .expect("viewer creates capability"),
    );
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    let invitation = viewer.capability_url(&format!("http://127.0.0.1:{port}/"));
    tokio::spawn(viewer.serve(listener));
    authorize(port, &invitation).await;

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
            urlencode("project:viewer-smoke:decision:first")
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

/// A parameter the caller got wrong is answered, not absorbed.
///
/// `depth=abc` used to come back 200 as though it read `depth=2`, while
/// `scope=nonsense` next door refused by name. This codebase argues the
/// opposite everywhere else — an unknown kind is refused rather than guessed —
/// and a typo that answers as if it were correct is the one failure a reader
/// cannot see.
#[tokio::test(flavor = "multi_thread")]
async fn a_parameter_that_is_not_a_number_is_refused_rather_than_defaulted() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel opens");
    kernel
        .service()
        .ingest(corpus())
        .await
        .expect("memory ingests");

    let viewer = Arc::new(
        MemoryViewerServer::new(
            kernel.service(),
            Some(data_dir.path().display().to_string()),
        )
        .expect("viewer creates capability"),
    );
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    let invitation = viewer.capability_url(&format!("http://127.0.0.1:{port}/"));
    tokio::spawn(viewer.serve(listener));
    authorize(port, &invitation).await;

    for (query, key) in [
        ("depth=abc", "depth"),
        ("budget=lots", "budget"),
        ("depth=2.5", "depth"),
    ] {
        let (status, body) = get(
            port,
            &format!("/api/graph?about={}&{query}", urlencode(ABOUT)),
        )
        .await;
        assert_eq!(status, 400, "`{query}` was absorbed: {body}");
        assert!(
            body["error"].as_str().is_some_and(|e| e.contains(key)),
            "the refusal must name the parameter, got {body}"
        );
    }

    // Out of range is still a policy, not a mistake: it clamps and answers.
    let (status, wide) = get(
        port,
        &format!("/api/graph?about={}&depth=9999", urlencode(ABOUT)),
    )
    .await;
    assert_eq!(status, 200, "an out-of-range depth clamps: {wide}");

    // And an absent parameter still means "use the default".
    let (status, plain) = get(port, &format!("/api/graph?about={}", urlencode(ABOUT))).await;
    assert_eq!(status, 200, "no parameters at all must still work: {plain}");
}

/// HEAD is GET without the body (RFC 9110 §9.3.2). It answered 405, so a
/// health check or link checker pointed at the viewer reported it down.
#[tokio::test(flavor = "multi_thread")]
async fn head_answers_the_same_head_as_get_and_no_body() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel opens");
    let viewer = Arc::new(
        MemoryViewerServer::new(kernel.service(), None).expect("viewer creates capability"),
    );
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    let invitation = viewer.capability_url(&format!("http://127.0.0.1:{port}/"));
    tokio::spawn(viewer.serve(listener));
    authorize(port, &invitation).await;

    let cookie = auth_cookie(port);
    let head = raw_request(
        port,
        &format!("HEAD / HTTP/1.1\r\nHost: 127.0.0.1\r\nCookie: {cookie}\r\n\r\n"),
    )
    .await;
    let get_response = raw_request(
        port,
        &format!("GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nCookie: {cookie}\r\n\r\n"),
    )
    .await;

    assert!(head.starts_with("HTTP/1.1 200"), "HEAD refused: {head}");
    let head_length = content_length(&head);
    assert_eq!(
        head_length,
        content_length(&get_response),
        "HEAD must report the length GET would send"
    );
    assert!(head_length > 0, "the page is not empty");
    let body = head.split("\r\n\r\n").nth(1).unwrap_or("");
    assert!(body.is_empty(), "HEAD sent a body: {body:?}");

    // Everything else is still refused.
    let post = raw_request(
        port,
        &format!("POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nCookie: {cookie}\r\n\r\n"),
    )
    .await;
    assert!(post.starts_with("HTTP/1.1 405"), "POST allowed: {post}");
}

fn content_length(response: &str) -> usize {
    response
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .and_then(|value| value.trim().parse().ok())
        .expect("a Content-Length header")
}

/// A GET has to be safe. If one could move the view, any page you happened
/// to be visiting could point an `<img>` at loopback and steer this loom.
#[tokio::test]
async fn a_get_cannot_move_the_view() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel opens");
    let viewer = Arc::new(
        MemoryViewerServer::new(kernel.service(), None).expect("viewer creates capability"),
    );
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    let invitation = viewer.capability_url(&format!("http://127.0.0.1:{port}/"));
    tokio::spawn(viewer.serve(listener));
    authorize(port, &invitation).await;

    let (status, opened) = post(port, "/api/view/open?id=safety&about=about:anything").await;
    assert_eq!(status, 200, "opening a view is a POST: {opened}");
    let before = opened["view_revision"].as_u64().expect("a revision");

    let (status, refusal) = get(port, "/api/view/report?id=safety&clock=ingested").await;
    assert_eq!(status, 405, "a GET must not change the view: {refusal}");

    let (status, after) = get(port, "/api/view?id=safety").await;
    assert_eq!(status, 200);
    assert_eq!(
        after["view_revision"].as_u64().expect("a revision"),
        before,
        "the refused GET left the view exactly where it was"
    );
    assert_eq!(after["clock"], "occurred", "and did not touch its clock");
}

async fn post(port: u16, path_and_query: &str) -> (u16, serde_json::Value) {
    let raw = raw_request(
        port,
        &format!(
            "POST {path_and_query} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: {}\r\nConnection: close\r\n\r\n",
            auth_cookie(port)
        ),
    )
    .await;
    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("a status code");
    let body = raw.split("\r\n\r\n").nth(1).unwrap_or("");
    (
        status,
        serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
    )
}

async fn raw_request(port: u16, request: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connects");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("request writes");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("response reads");
    String::from_utf8_lossy(&raw).to_string()
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
