//! An old source does not backdate a newly declared evidence association (#701).
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:attachment-clock";
const ENTRY: &str = "project:attachment-clock:observation:execution";
const SOURCE: &str = "evidence:project:attachment-clock:old-log";
const EARLY: &str = "2026-09-01T10:00:00Z";
const LATE: &str = "2026-09-02T13:00:00.500Z";

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let wire = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0","id":1,
        "method":"tools/call","params":{"name":tool,"arguments":arguments}})
            .to_string(),
        )
        .await
        .expect("response");
    let response: Value = serde_json::from_str(&wire).expect("JSON");
    assert_eq!(response["result"]["isError"], false, "{response}");
    response["result"]["structuredContent"].clone()
}

fn packet(later: bool) -> Value {
    json!({"about":ABOUT,"idempotency_key":if later {"attachment"} else {"source"},
        "provenance":{"source_kind":"agent","source_agent":"investigator",
            "observed_at":if later {LATE} else {EARLY}},
        "memory":{"dimensions":[{"id":"audit","kind":"task"}],"entries":[{
            "id":if later {"project:attachment-clock:observation:review"} else {ENTRY},
            "kind":"observation","text":if later {"Review associates the log with the execution."} else {"The execution took place."},
            "coordinates":[{"dimension":"task","scope_id":"audit","observed_at":if later {LATE} else {EARLY},"occurred_at":if later {LATE} else {EARLY}}]}],
            "evidence":[{"id":SOURCE,"text":"Old log records check code Z17.",
                "source":"fixture:R1","time":EARLY,"supports":if later {vec![ENTRY]} else {vec![]}}]}})
}

fn scratch() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&root).expect("scratch root");
    tempfile::tempdir_in(root).expect("isolated store")
}

async fn assert_support(server: &KernelMcpServer, cut: &str, expected: bool) {
    for tool in ["kmp_wake", "kmp_goto"] {
        let mut args = json!({"about":ABOUT,"axis":"observed","budget":{"max_bytes":200000}});
        if tool == "kmp_wake" {
            args["as_of"] = json!({"time":cut});
        } else {
            args["at"] = json!({"time":cut});
            args["refs"] = json!([ENTRY]);
            args["include"] = json!({"evidence":true,"relations":true});
        }
        let result = call(server, tool, args).await;
        let evidence = result["proof"]["evidence"].as_array().expect("evidence");
        assert_eq!(
            evidence
                .iter()
                .any(|e| e["id"] == format!("detail:{SOURCE}")),
            expected,
            "{tool} at {cut}: {result}"
        );
    }
}

#[tokio::test]
async fn old_source_gains_support_only_when_the_association_is_observed() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    call(&server, "kmp_ingest", packet(false)).await;
    assert_support(&server, "2026-09-02T13:00:00.499Z", false).await;
    call(&server, "kmp_ingest", packet(true)).await;
    assert_support(&server, "2026-09-02T13:00:00.499Z", false).await;
    assert_support(&server, LATE, true).await;
    assert_support(&server, "2026-09-02T13:00:00.501Z", true).await;
}

async fn read_source(server: &KernelMcpServer, axis: &str, cut: &str, interval: bool) -> Value {
    let mut args = json!({"about":ABOUT,"axis":axis,"budget":{"max_bytes":200000},
        "include":{"evidence":true,"relations":true}});
    let tool = if interval {
        args["interval"] = json!({"end":cut});
        "kmp_forward"
    } else {
        args["at"] = json!({"time":cut});
        args["refs"] = json!([ENTRY]);
        "kmp_goto"
    };
    let result = call(server, tool, args).await;
    result["proof"]["evidence"]
        .as_array()
        .expect("evidence")
        .iter()
        .find(|e| e["id"] == format!("detail:{SOURCE}"))
        .cloned()
        .unwrap_or(Value::Null)
}

#[tokio::test]
async fn attachment_clocks_preserve_source_time_boundaries_retry_restart_and_restore() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    call(&server, "kmp_ingest", packet(false)).await;
    call(&server, "kmp_ingest", packet(true)).await;
    let source = read_source(&server, "observed", LATE, false).await;
    assert_eq!(source["time"], EARLY);
    assert_eq!(source["support_clocks"]["observed_at"], LATE);
    let ingested = source["support_clocks"]["ingested_at"]
        .as_str()
        .expect("ingestion");
    assert!(
        !read_source(&server, "ingested", ingested, false)
            .await
            .is_null()
    );
    assert!(
        read_source(&server, "ingested", ingested, true)
            .await
            .is_null(),
        "exclusive ingestion end"
    );
    assert!(
        read_source(&server, "observed", LATE, true).await.is_null(),
        "exclusive observation end"
    );
    assert!(
        !read_source(&server, "occurred", EARLY, false)
            .await
            .is_null(),
        "event time is not knowledge time"
    );
    call(&server, "kmp_ingest", packet(true)).await;
    assert_eq!(read_source(&server, "observed", LATE, false).await, source);
    drop(server);
    let reopened = KernelMcpServer::embedded(dir.path()).expect("reopened");
    assert_eq!(
        read_source(&reopened, "observed", LATE, false).await,
        source
    );
    let restored_dir = scratch();
    let restored = KernelMcpServer::embedded(restored_dir.path()).expect("restored");
    call(&restored, "kmp_ingest", packet(false)).await;
    let mut restore = packet(true);
    restore["memory"]["evidence"][0]["support_clocks"] = source["support_clocks"].clone();
    call(&restored, "kmp_ingest", restore).await;
    assert_eq!(
        read_source(&restored, "observed", LATE, false).await,
        source
    );
}

#[tokio::test]
async fn implicit_attachment_observation_uses_ingestion_and_restored_unknown_stays_unknown() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    call(&server, "kmp_ingest", packet(false)).await;
    let mut request = packet(true);
    request
        .as_object_mut()
        .expect("object")
        .remove("provenance");
    call(&server, "kmp_ingest", request).await;
    let source = read_source(&server, "occurred", EARLY, false).await;
    let clocks = &source["support_clocks"];
    assert_eq!(clocks["observed_at"], clocks["ingested_at"]);
    assert!(clocks["ingested_at"].is_string());
    let other_dir = scratch();
    let restored = KernelMcpServer::embedded(other_dir.path()).expect("restored");
    call(&restored, "kmp_ingest", packet(false)).await;
    let mut historical = packet(true);
    historical["memory"]["evidence"][0]["support_clocks"] = json!({"ingested_at":LATE});
    call(&restored, "kmp_ingest", historical).await;
    let source = read_source(&restored, "occurred", EARLY, false).await;
    assert!(source["support_clocks"].get("observed_at").is_none());
    assert_eq!(source["support_clocks"]["ingested_at"], LATE);
}

#[tokio::test]
async fn ask_candidates_cannot_use_an_association_from_after_the_observed_cut() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    call(&server, "kmp_ingest", packet(false)).await;
    call(&server, "kmp_ingest", packet(true)).await;
    for (cut, known) in [("2026-09-02T13:00:00.499Z", false), (LATE, true)] {
        let result = call(
            &server,
            "kmp_ask",
            json!({"about":ABOUT,"axis":"observed",
            "as_of":{"time":cut},"question":"Which check code does the old log record?",
            "budget":{"max_bytes":200000}}),
        )
        .await;
        assert_eq!(
            result["proof"]["evidence"]
                .as_array()
                .expect("evidence")
                .iter()
                .any(|e| e["id"] == format!("detail:{SOURCE}")),
            known,
            "{result}"
        );
    }
}
