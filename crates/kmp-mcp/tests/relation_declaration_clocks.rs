//! A later verification must not enter an earlier historical proof merely
//! because both endpoints already existed. All calls use the native backend.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:relation-clock";
const EARLY: &str = "2026-09-01T10:00:00Z";
const LATE: &str = "2026-09-02T13:00:00.500Z";

async fn call(server: &KernelMcpServer, tool: &str, args: Value) -> Value {
    let wire = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":args}})
            .to_string(),
        )
        .await
        .expect("valid clock fixture");
    let reply: Value = serde_json::from_str(&wire).expect("valid clock fixture");
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    reply["result"]["structuredContent"].clone()
}

fn scratch() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&root).expect("valid clock fixture");
    tempfile::tempdir_in(root).expect("valid clock fixture")
}

async fn seed(server: &KernelMcpServer) -> Value {
    call(server, "kmp_write_memory", json!({"about":ABOUT,"actor":"investigator",
        "idempotency_key":"old-records","observed_at":EARLY,"occurred_at":EARLY,
        "labels":{"task":["deployment"]},"memories":[
            {"id":"execution","kind":"observation","summary":"The deployment ran.","evidence":"R1 records an execution; success has not been verified."},
            {"id":"check","kind":"observation","summary":"The check log is available.","evidence":"R2 records the check output; its connection to this deployment is not yet known."}
        ]})).await["local_refs"].clone()
}

fn declaration(refs: &Value) -> Value {
    json!({"about":ABOUT,"idempotency_key":"later-verification",
        "memory":{"dimensions":[{"id":"deployment","kind":"task"}],"entries":[{
            "id":"project:relation-clock:observation:review","kind":"observation","text":"R3 connects the earlier check to the execution.",
            "coordinates":[{"dimension":"task","scope_id":"deployment","observed_at":LATE}]}],"relations":[{
            "from":refs["check"],"to":refs["execution"],"rel":"supports","class":"evidential",
            "confidence":"high","why":"The review identifies the check as verification of this deployment.",
            "evidence":"R3 received at 13:00:00.500 connects the check log to the deployment."}]},
        "provenance":{"source_kind":"agent","source_agent":"reviewer","observed_at":LATE}})
}

#[tokio::test]
async fn old_endpoints_do_not_backdate_a_new_relation_in_trace_or_wake() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("valid clock fixture");
    let refs = seed(&server).await;
    let result = call(&server, "kmp_ingest", declaration(&refs)).await;
    assert_eq!(
        result["memory"]["clocks"]["relations"]["observed"], 1,
        "{result}"
    );
    for (cut, present) in [("2026-09-02T13:00:00.499Z", false), (LATE, true)] {
        let trace = call(
            &server,
            "kmp_trace",
            json!({"about":ABOUT,"from":refs["check"],
            "to":[refs["execution"]],"as_of":{"time":cut},"axis":"observed",
            "budget":{"max_bytes":200000}}),
        )
        .await;
        assert_eq!(
            trace["routes"]
                .as_array()
                .expect("valid clock fixture")
                .len(),
            usize::from(present),
            "{trace}"
        );
        assert_eq!(trace["search"]["clock_unknown_edges"], json!([]), "{trace}");
        if present {
            assert_eq!(trace["trace"][0]["clocks"]["observed_at"], LATE);
            assert!(trace["trace"][0]["clocks"]["ingested_at"].is_string());
            assert!(trace["trace"][0].get("coordinate").is_none());
            assert!(trace["trace"][0]["clocks"].get("occurred_at").is_none());
        }
        let wake = call(
            &server,
            "kmp_wake",
            json!({"about":ABOUT,"as_of":{"time":cut},
            "axis":"observed","budget":{"max_bytes":200000},"depth":5}),
        )
        .await;
        let links = wake["proof"]["path"]
            .as_array()
            .expect("valid clock fixture");
        let declaration = links
            .iter()
            .find(|r| r["rel"] == "supports" && r["from"] == refs["check"]);
        assert_eq!(declaration.is_some(), present, "{wake}");
        if let Some(link) = declaration {
            assert_eq!(link["clocks"]["observed_at"], LATE);
        }
    }
    let span = call(
        &server,
        "kmp_trace",
        json!({"about":ABOUT,"from":refs["check"],
        "to":[refs["execution"]],"interval":{"end":LATE},"axis":"observed",
        "budget":{"max_bytes":200000}}),
    )
    .await;
    assert_eq!(span["routes"], json!([]), "exclusive upper boundary");
}

#[tokio::test]
async fn explicit_relation_clocks_survive_canonical_ingest_without_a_label() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("valid clock fixture");
    let refs = seed(&server).await;
    let mut request = declaration(&refs);
    let clocks = json!({"occurred_at":EARLY,"observed_at":LATE,
        "ingested_at":"2026-09-03T09:00:00Z","valid_from":LATE,"valid_until":"2026-09-04T10:00:00Z"});
    request["memory"]["relations"][0]["clocks"] = clocks.clone();
    call(&server, "kmp_ingest", request).await;
    let trace = call(
        &server,
        "kmp_trace",
        json!({"about":ABOUT,"from":refs["check"],
        "to":refs["execution"],"budget":{"max_bytes":200000}}),
    )
    .await;
    assert_eq!(trace["trace"][0]["clocks"], clocks);
    assert!(trace["trace"][0].get("coordinate").is_none());
}
