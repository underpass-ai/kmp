//! A batch member declares its proof at its own observation, not a neighbor's.
#[path = "support/write_neighborhood_fixture.rs"]
mod fixture;

use fixture::{call, resume, scratch};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

fn assert_same_time(actual: &Value, expected: &str) {
    assert_eq!(
        kmp_domain::compare_temporal_instants(actual.as_str().expect("clock"), expected),
        Some(std::cmp::Ordering::Equal),
        "{actual} != {expected}"
    );
}

const ABOUT: &str = "project:batch-proof-clock";
const EARLY: &str = "2026-09-01T10:00:00Z";
const LATE: &str = "2026-09-02T14:00:00Z";

fn packet() -> Value {
    json!({"about":ABOUT,"actor":"investigator","idempotency_key":"proof-clock",
        "labels":{"task":["clock-proof"]},"memories":[
            {"id":"check","kind":"observation","summary":"V41 measured 0.6 degrees error.",
                "evidence":"Log V41: absolute error 0.6 degrees."},
            {"id":"claim","kind":"observation","summary":"The check confirms the measurement.",
                "evidence":"Report R1 cites V41's measurement.",
                "connect_to":[{"ref":"@check","rel":"verified_by",
                    "why":"The report is verified by the recorded measurement.",
                    "evidence":"Report R1 cites log V41: 0.6 degrees."}]}]})
}

async fn write(server: &KernelMcpServer, args: Value) -> Value {
    let result = call(server, "kmp_write_memory", args).await;
    if result["status"] == "needs_review" {
        resume(server, &result).await
    } else {
        result
    }
}

async fn receipt(server: &KernelMcpServer, written: &Value) -> Value {
    let action = &written["receipt"]["action"];
    let inspected = call(server, "kmp_inspect", action["arguments"].clone()).await;
    serde_json::from_str::<Value>(inspected["object"]["text"].as_str().expect("receipt"))
        .expect("JSON")["receipt"]
        .clone()
}

fn assert_member_proof(stored: &Value, expected: &[&str; 2]) {
    let memory = &stored["canonical_memory"];
    for (index, entry) in memory["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .enumerate()
    {
        assert_same_time(&entry["coordinates"][0]["observed_at"], expected[index]);
    }
    let relation = &memory["relations"][0];
    assert_same_time(&relation["clocks"]["observed_at"], expected[1]);
    assert!(relation["clocks"].get("occurred_at").is_none());
    assert!(relation["clocks"].get("valid_from").is_none());
    let evidence = memory["evidence"].as_array().expect("evidence");
    assert_eq!(evidence.len(), 3);
    for (item, expected) in evidence.iter().zip([expected[0], expected[1], expected[1]]) {
        assert_same_time(&item["time"], expected);
        assert_same_time(&item["support_clocks"]["observed_at"], expected);
        assert_eq!(
            item["support_clocks"]["ingested_at"],
            relation["clocks"]["ingested_at"]
        );
    }
}

#[tokio::test]
async fn generated_proof_preserves_packet_member_mixed_and_null_observations() {
    for (packet_time, first, second) in [
        (Some(EARLY), None, None),
        (None, Some(json!(EARLY)), Some(json!(EARLY))),
        (Some(EARLY), None, Some(json!(LATE))),
        (Some(EARLY), None, Some(Value::Null)),
        (None, Some(json!(EARLY)), None),
    ] {
        let dir = scratch();
        let server = KernelMcpServer::embedded(dir.path()).expect("server");
        let mut args = packet();
        if let Some(at) = packet_time {
            args["observed_at"] = json!(at);
        }
        for (index, time) in [&first, &second].into_iter().enumerate() {
            if let Some(time) = time {
                args["memories"][index]["observed_at"] = time.clone();
            }
        }
        let written = write(&server, args.clone()).await;
        assert_eq!(written["status"], "committed", "{written}");
        let stored = receipt(&server, &written).await;
        let ingestion = stored["canonical_memory"]["entries"][0]["coordinates"][0]["ingested_at"]
            .as_str()
            .expect("kernel ingestion");
        let expected = [&first, &second].map(|at| match at {
            Some(value) => value.as_str().unwrap_or(ingestion),
            None => packet_time.unwrap_or(ingestion),
        });
        assert_member_proof(&stored, &expected);
        assert_same_time(
            &stored["provenance"]["observed_at"],
            packet_time.unwrap_or(ingestion),
        );
        let replay = write(&server, args).await;
        assert_eq!(replay["status"], "replayed");
        assert_eq!(receipt(&server, &replay).await, stored);
    }
}

async fn historical_proof(server: &KernelMcpServer, at: &str, exclusive: bool) -> Value {
    let mut arguments = json!({"about":ABOUT,"axis":"observed",
        "budget":{"detail":"full","max_bytes":200000},
        "include":{"evidence":true,"relations":true},"limit":{"entries":100}});
    let tool = if exclusive {
        arguments["interval"] = json!({"end":at});
        "kmp_forward"
    } else {
        arguments["at"] = json!({"time":at});
        arguments["window"] = json!({"before_entries":100,"after_entries":0});
        "kmp_goto"
    };
    let result = call(server, tool, arguments).await;
    assert_ne!(result["projection"]["page"]["has_more"], true, "{result}");
    result["proof"].clone()
}

#[tokio::test]
async fn historical_member_proof_survives_restart_and_bundle_import() {
    use kmp_adapter_embedded::EmbeddedKernelStore;

    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut args = packet();
    for member in args["memories"].as_array_mut().expect("members") {
        member["observed_at"] = json!(EARLY);
    }
    let written = write(&server, args.clone()).await;
    let stored = receipt(&server, &written).await;
    let proof = historical_proof(&server, EARLY, false).await;
    let sources = proof["evidence"].as_array().expect("sources");
    for source in stored["canonical_memory"]["evidence"]
        .as_array()
        .expect("canonical sources")
    {
        let actual = sources
            .iter()
            .find(|e| e["id"] == format!("detail:{}", source["id"].as_str().expect("source id")))
            .expect("declared source available at inclusive boundary");
        assert_eq!(actual["text"], source["text"]);
        assert_same_time(&actual["support_clocks"]["observed_at"], EARLY);
    }
    assert!(
        proof["path"]
            .as_array()
            .expect("links")
            .iter()
            .any(|r| r["rel"] == "verified_by")
    );
    let before = historical_proof(&server, EARLY, true).await;
    assert!(
        !before["path"]
            .as_array()
            .expect("links")
            .iter()
            .any(|r| r["rel"] == "verified_by")
    );
    assert!(before["evidence"].as_array().expect("sources").is_empty());
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let bundle = store.export_bundle().await.expect("export");
    drop(server);
    let reopened = KernelMcpServer::embedded(dir.path()).expect("restart");
    assert_eq!(historical_proof(&reopened, EARLY, false).await, proof);

    let restored_dir = scratch();
    let restored_store = EmbeddedKernelStore::open(restored_dir.path()).expect("restored store");
    restored_store
        .import_bundle(
            &bundle,
            kmp_application::projection_mutations_for_context_event,
        )
        .await
        .expect("import");
    let restored = KernelMcpServer::embedded(restored_dir.path()).expect("restored server");
    assert_eq!(historical_proof(&restored, EARLY, false).await, proof);
    let replay = write(&restored, args).await;
    assert_eq!(replay["status"], "replayed");
    assert_eq!(receipt(&restored, &replay).await, stored);
}
