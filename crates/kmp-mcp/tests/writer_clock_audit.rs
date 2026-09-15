//! A source-backed control keeps execution, verification, reception and a
//! later evidence association on their own clocks (#653).
//!
//! This is an authored kernel control, not evidence that a separate writer
//! understands the source. The matching fresh-writer protocol lives in
//! `artifacts/writer-clocks-20260915/`.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:writer-clock-audit";
const REPORT_OBSERVED: &str = "2026-09-04T12:00:00Z";
const ASSOCIATION_OBSERVED: &str = "2026-09-04T12:10:00Z";
const EXECUTION_OCCURRED: &str = "2026-09-04T11:00:00Z";
const VERIFICATION_OCCURRED: &str = "2026-09-04T11:05:00Z";
const LATE_SOURCE: &str = "evidence:project:writer-clock-audit:f17";

fn scratch() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&root).expect("scratch root");
    tempfile::tempdir_in(root).expect("isolated store")
}

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

async fn resume(server: &KernelMcpServer, pending: &Value) -> Value {
    assert_eq!(pending["status"], "needs_review", "{pending}");
    call(
        server,
        pending["next_actions"][0]["tool"].as_str().expect("tool"),
        pending["next_actions"][0]["arguments"].clone(),
    )
    .await
}

fn write_packet() -> Value {
    json!({"about":ABOUT,"actor":"investigator","idempotency_key":"f17-report",
    "observed_at":REPORT_OBSERVED,"labels":{"backup":["R8"]},
    "memories":[
        {"id":"execution","kind":"observation","labels":{"source":["F17"]},"occurred_at":EXECUTION_OCCURRED,
         "summary":"R8 ran in staging at 11:00.",
         "evidence":"F17: R8 ran in staging at 2026-09-04T11:00:00Z.",
         "connect_to":[{"ref":"@verification","rel":"verified_by","class":"evidential",
             "why":"The later H8 check verifies the R8 execution outcome.",
             "evidence":"F17 separately reports R8 at 11:00 and H8 at 11:05.",
             "confidence":"high"}]},
        {"id":"verification","kind":"observation","labels":{"source":["F17"]},"occurred_at":VERIFICATION_OCCURRED,
         "summary":"H8 checked the restored file at 11:05.",
         "evidence":"F17: H8 checked the restored file at 2026-09-04T11:05:00Z."},
        {"id":"quantity81","kind":"observation","labels":{"source":["F17"]},
         "summary":"The copied quantity was recorded as 81 MB.",
         "evidence":"F17 correction identifies the earlier copied quantity as 81 MB."},
        {"id":"correction","kind":"observation","labels":{"source":["F17"]},
         "occurred_at":"2026-09-04T11:06:00Z",
         "summary":"F17 corrects the copied quantity from 81 MB to 80 MB.",
         "evidence":"F17 correction: the copied quantity is 80 MB, not 81 MB.",
         "connect_to":[{"ref":"@quantity81","rel":"corrects","class":"evidential",
             "why":"The correction changes the copied quantity only.",
             "evidence":"F17 says 80 MB, not 81 MB.","confidence":"high"}]},
        {"id":"conflict","kind":"observation","observed_at":null,
         "occurred_at":"2026-09-04T11:07:00Z","labels":{"source":["G2"]},
         "summary":"G2 reports 79 MB; the quantity remains unresolved.",
         "evidence":"G2 reports 79 MB, conflicting with F17's 80 MB.",
         "connect_to":[{"ref":"@correction","rel":"contradicts","class":"evidential",
             "why":"G2 reports a different quantity for the same copy.",
             "evidence":"F17: 80 MB; G2: 79 MB.","confidence":"high"}]},
    ]})
}

fn has_ref(result: &Value, reference: &Value) -> bool {
    result["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .any(|entry| entry["ref"] == *reference)
}

async fn goto(server: &KernelMcpServer, axis: &str, time: &str) -> Value {
    call(
        server,
        "kmp_goto",
        json!({"about":ABOUT,"axis":axis,"at":{"time":time},
            "include":{"evidence":true,"relations":true},
            "limit":{"entries":100},"budget":{"max_bytes":200000}}),
    )
    .await
}

async fn support_at(
    server: &KernelMcpServer,
    execution: &Value,
    axis: &str,
    time: &str,
    interval: bool,
) -> Value {
    let mut arguments = json!({"about":ABOUT,"axis":axis,
        "include":{"evidence":true,"relations":true},"budget":{"max_bytes":200000}});
    let tool = if interval {
        arguments["interval"] = json!({"end":time});
        "kmp_forward"
    } else {
        arguments["at"] = json!({"time":time});
        arguments["refs"] = json!([execution]);
        "kmp_goto"
    };
    let result = call(server, tool, arguments).await;
    result["proof"]["evidence"]
        .as_array()
        .expect("evidence")
        .iter()
        .find(|item| item["id"] == format!("detail:{LATE_SOURCE}"))
        .cloned()
        .unwrap_or(Value::Null)
}

#[tokio::test]
async fn source_backed_clock_control_keeps_event_knowledge_and_late_support_separate() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded server");
    let pending = call(&server, "kmp_write_memory", write_packet()).await;
    assert_eq!(pending["accepted"], false, "{pending}");
    let neighborhood = pending["neighborhood"].to_string();
    assert!(!neighborhood.contains("unix:"), "{neighborhood}");

    let written = resume(&server, &pending).await;
    assert_eq!(written["status"], "committed", "{written}");
    let refs = &written["local_refs"];
    let occurred_before = goto(&server, "occurred", "2026-09-04T11:02:00Z").await;
    assert!(has_ref(&occurred_before, &refs["execution"]));
    assert!(!has_ref(&occurred_before, &refs["verification"]));
    assert!(
        !occurred_before["proof"]
            .to_string()
            .contains(refs["verification"].as_str().expect("verification ref")),
        "the later check is absent from the selected proof route, not merely from entries: {occurred_before}"
    );
    let occurred_equal = goto(&server, "occurred", VERIFICATION_OCCURRED).await;
    assert!(has_ref(&occurred_equal, &refs["execution"]));
    assert!(has_ref(&occurred_equal, &refs["verification"]));

    let observed_report = goto(&server, "observed", REPORT_OBSERVED).await;
    assert!(has_ref(&observed_report, &refs["execution"]));
    assert!(has_ref(&observed_report, &refs["verification"]));
    assert!(has_ref(&observed_report, &refs["correction"]));
    assert!(
        !has_ref(&observed_report, &refs["conflict"]),
        "G2 has no supplied observation and therefore remains at its own ingestion: {observed_report}"
    );
    let observed_before = goto(&server, "observed", "2026-09-04T11:02:00Z").await;
    assert_eq!(
        observed_before["entries"],
        json!([]),
        "the source was first observed at noon, so neither event is known at 11:02: {observed_before}"
    );
    let receipt = call(
        &server,
        "kmp_inspect",
        written["receipt"]["action"]["arguments"].clone(),
    )
    .await;
    let canonical: Value =
        serde_json::from_str(receipt["object"]["text"].as_str().expect("receipt text"))
            .expect("receipt JSON");
    let relations = canonical["receipt"]["canonical_memory"]["relations"]
        .as_array()
        .expect("relations");
    let has_relation = |from: &Value, rel: &str, to: &Value| {
        relations.iter().any(|relation| {
            relation["from"] == *from && relation["rel"] == rel && relation["to"] == *to
        })
    };
    assert!(has_relation(
        &refs["execution"],
        "verified_by",
        &refs["verification"]
    ));
    assert!(has_relation(
        &refs["correction"],
        "corrects",
        &refs["quantity81"]
    ));
    assert!(has_relation(
        &refs["conflict"],
        "contradicts",
        &refs["correction"]
    ));
    assert!(
        relations
            .iter()
            .all(|relation| relation["rel"] != "supersedes"),
        "the source supplies no whole-record replacement"
    );
    let verification_ingested = canonical["receipt"]["canonical_memory"]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["id"] == refs["verification"])
        .expect("verification entry")["coordinates"][0]["ingested_at"]
        .as_str()
        .expect("ingestion")
        .to_owned();
    let verification_ingested = kmp_domain::temporal_instant_rfc3339(&verification_ingested)
        .expect("canonical ingestion instant");
    let ingested_equal = goto(&server, "ingested", &verification_ingested).await;
    assert!(has_ref(&ingested_equal, &refs["verification"]));

    // This association has an old source time but is explicitly declared later.
    call(
        &server,
        "kmp_ingest",
        json!({"about":ABOUT,"idempotency_key":"f17-association",
        "provenance":{"source_kind":"human","source_agent":"investigator",
            "observed_at":ASSOCIATION_OBSERVED},
        "memory":{"dimensions":[{"id":"backup","kind":"backup"}],"entries":[{
            "id":"project:writer-clock-audit:observation:association","kind":"observation",
            "text":"At 12:10, the investigator associates F17 with R8.",
            "coordinates":[{"dimension":"backup","scope_id":"backup",
                "observed_at":ASSOCIATION_OBSERVED}]}],"evidence":[{
            "id":LATE_SOURCE,"supports":[refs["execution"]],"text":"F17 records R8 and H8.",
            "source":"fixture:F17","time":EXECUTION_OCCURRED}]}}),
    )
    .await;
    assert!(
        support_at(
            &server,
            &refs["execution"],
            "observed",
            "2026-09-04T12:09:59Z",
            false
        )
        .await
        .is_null()
    );
    let support = support_at(
        &server,
        &refs["execution"],
        "observed",
        ASSOCIATION_OBSERVED,
        false,
    )
    .await;
    assert_eq!(support["time"], EXECUTION_OCCURRED);
    assert_eq!(
        support["support_clocks"]["observed_at"],
        ASSOCIATION_OBSERVED
    );
    assert!(
        support_at(
            &server,
            &refs["execution"],
            "observed",
            ASSOCIATION_OBSERVED,
            true
        )
        .await
        .is_null(),
        "the interval end is exclusive"
    );
    let association_ingested = kmp_domain::temporal_instant_rfc3339(
        support["support_clocks"]["ingested_at"]
            .as_str()
            .expect("association ingestion"),
    )
    .expect("canonical association ingestion");
    assert!(
        !support_at(
            &server,
            &refs["execution"],
            "ingested",
            &association_ingested,
            false,
        )
        .await
        .is_null(),
        "the association is admitted at its inclusive ingestion boundary"
    );
    assert!(
        support_at(
            &server,
            &refs["execution"],
            "ingested",
            &association_ingested,
            true,
        )
        .await
        .is_null(),
        "the ingestion interval end is exclusive"
    );
}
