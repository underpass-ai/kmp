//! A compact acknowledgment must point to durable, immutable accepted detail.
#[path = "support/reviewed_writer.rs"]
mod reviewed_writer;
use kmp_adapter_embedded::{EmbeddedKernelStore, verify_bundle};
use kmp_application::projection_mutations_for_context_event;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:receipt-check";

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}})
    .to_string();
    let result = server.handle_json_line(&line).await.expect("response");
    let result: Value = serde_json::from_str(&result).expect("JSON");
    assert!(result.get("error").is_none(), "{result}");
    reviewed_writer::review_authored_write(server, result["result"].clone()).await
}

async fn inspect_action(server: &KernelMcpServer, action: &Value) -> Value {
    let response = call(
        server,
        action["tool"].as_str().expect("tool"),
        action["arguments"].clone(),
    )
    .await;
    assert_eq!(response["isError"], false, "{response}");
    assert_eq!(
        response["structuredContent"]["object"]["kind"],
        "write_receipt"
    );
    serde_json::from_str(
        response["structuredContent"]["object"]["text"]
            .as_str()
            .expect("detail"),
    )
    .expect("receipt JSON")
}

fn packet() -> Value {
    json!({
        "about":ABOUT,"actor":"writer-check","observed_at":"2026-09-01T11:00:00Z",
        "idempotency_key":"receipt:write:one", "labels":{"component":["gateway"]},
        "memories":[
            {"id":"log","kind":"observation","summary":"Requests fail immediately after token refresh.",
             "evidence":"R1 records successful refresh followed by an unauthorized request. ".repeat(100),
             "labels":{"alias":["Orion Gateway","OGW"]}},
            {"id":"decision","kind":"decision","summary":"Retry the request after token refresh.",
             "evidence":"R2 selects retry to address R1.","occurred_at":"2026-09-01T10:00:00Z",
             "connect_to":[{"ref":"@log","rel":"chosen_because","class":"causal",
                "why":"Retry addresses the failure after refresh.","evidence":"R2 explicitly cites R1."}]}
        ]
    })
}

#[tokio::test]
async fn receipt_preserves_accepted_proof_through_restart_later_change_and_bundle_import() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store reader");
    let written = call(&server, "kmp_write_memory", packet()).await;
    assert_eq!(written["isError"], false, "{written}");
    let ack = &written["structuredContent"];
    assert_eq!(ack["accepted"], true);
    assert_eq!(ack["read_after_write_ready"], true);
    assert_eq!(ack["coverage"]["label_memberships"], 4);
    assert_eq!(ack["coverage"]["source_coverage"], "not_assessed");
    assert!(ack.get("ingest_result").is_none());
    assert!(ack.get("relation_quality").is_none());
    let action = ack["receipt"]["action"].clone();
    let original = inspect_action(&server, &action).await;
    assert_eq!(
        original["receipt"]["writer"]["local_refs"],
        ack["local_refs"]
    );
    assert_eq!(original["receipt"]["writer"]["coverage"], ack["coverage"]);
    let memory = &original["receipt"]["canonical_memory"];
    assert!(
        memory["evidence"]
            .as_array()
            .expect("proof")
            .iter()
            .any(|proof| proof["text"].as_str()
                == packet()["memories"][0]["evidence"].as_str().map(str::trim))
    );
    assert_eq!(
        memory["relations"][0]["why"],
        "Retry addresses the failure after refresh."
    );
    let coordinates = memory["entries"][0]["coordinates"]
        .as_array()
        .expect("memberships");
    assert_eq!(coordinates.len(), 3);
    assert!(
        coordinates
            .iter()
            .all(|coordinate| coordinate["ingested_at"].is_string()
                && coordinate["sequence"].is_u64())
    );
    assert!(
        ack.to_string().len() < original.to_string().len() / 2,
        "ack should omit source/proof repetition"
    );
    let replay = call(&server, "kmp_write_memory", packet()).await;
    assert_eq!(replay["structuredContent"]["receipt"], ack["receipt"]);
    assert_eq!(
        verify_bundle(&store.export_bundle().await.expect("bundle"))
            .expect("verify")
            .event_count,
        1
    );
    let update = call(&server, "kmp_write_memory", json!({
        "about":ABOUT,"actor":"writer-check","observed_at":"2026-09-02T11:00:00Z",
        "idempotency_key":"receipt:write:summary", "search_summaries":[{
            "ref":ack["local_refs"]["log"],"summary_en":"Token renewal succeeds but the next request fails immediately."
        }]
    })).await;
    assert_eq!(update["isError"], false, "{update}");
    assert_eq!(inspect_action(&server, &action).await, original);
    let summary_detail =
        inspect_action(&server, &update["structuredContent"]["receipt"]["action"]).await;
    assert_eq!(
        summary_detail["receipt"]["writer"]["coverage"]["search_summaries"],
        1
    );
    let wake = call(&server, "kmp_wake", json!({"about":ABOUT})).await;
    assert_eq!(wake["isError"], false);
    assert!(
        !wake.to_string().contains("receipt:v1:"),
        "receipts are not graph memories"
    );
    let bundle = store.export_bundle().await.expect("bundle");
    drop(server);
    drop(store);
    let reopened = KernelMcpServer::embedded(dir.path()).expect("reopen");
    assert_eq!(inspect_action(&reopened, &action).await, original);

    let target_dir = tempfile::tempdir().expect("target");
    let target = EmbeddedKernelStore::open(target_dir.path()).expect("target store");
    target
        .import_bundle(&bundle, projection_mutations_for_context_event)
        .await
        .expect("import");
    let imported = KernelMcpServer::embedded(target_dir.path()).expect("imported server");
    assert_eq!(inspect_action(&imported, &action).await, original);
    let mut foreign = action["arguments"].clone();
    foreign["about"] = json!("project:other");
    let refused = call(&imported, "kmp_inspect", foreign).await;
    assert_eq!(refused["isError"], true);
    assert!(!refused.to_string().contains("R1 records"));
}

#[tokio::test]
async fn previews_and_refusals_leave_no_receipt_or_event() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let mut args = packet();
    args["options"] = json!({"dry_run":true});
    let preview = call(&server, "kmp_write_memory", args.clone()).await;
    assert_eq!(preview["isError"], false, "{preview}");
    assert!(preview["structuredContent"].get("receipt").is_none());
    args["options"]["dry_run"] = json!(false);
    args["memories"][1]["connect_to"][0]["ref"] = json!("project:receipt-check:entry:missing");
    args["read_context"] = json!({"inspected_refs":["project:receipt-check:entry:missing"]});
    let refused = call(&server, "kmp_write_memory", args).await;
    assert_eq!(refused["isError"], true, "{refused}");
    let receipt = kmp_domain::MemoryReceiptRef::new(ABOUT, "receipt:write:one").expect("ref");
    let absent = call(
        &server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":receipt.reference()}),
    )
    .await;
    assert_eq!(absent["isError"], true);
    assert_eq!(absent["structuredContent"]["error"]["code"], "not_found");
    assert_eq!(
        verify_bundle(&store.export_bundle().await.expect("bundle"))
            .expect("verify")
            .event_count,
        0
    );
}

#[tokio::test]
async fn compact_acceptance_retains_unverified_relation_warning_and_an_executable_action() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let seed = call(&server, "kmp_write_memory", packet()).await;
    assert_eq!(seed["isError"], false, "{seed}");
    let mut next = packet();
    next["idempotency_key"] = json!("receipt:unverified");
    next["options"] = json!({"strict": false});
    next["memories"][1]["connect_to"][0]["ref"] =
        seed["structuredContent"]["local_refs"]["log"].clone();
    let written = call(&server, "kmp_write_memory", next).await;
    assert_eq!(written["structuredContent"]["accepted"], true, "{written}");
    let feedback = &written["structuredContent"]["feedback"][0];
    assert_eq!(feedback["code"], "RELATION_CONTEXT_UNVERIFIED");
    assert_eq!(feedback["severity"], "warning");
    let detail = inspect_action(&server, &feedback["action"]).await;
    assert_eq!(
        detail["receipt"]["writer"]["relation_quality_metrics"]["relation_suspect_count"],
        1
    );
}
