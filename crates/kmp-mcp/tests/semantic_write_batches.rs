//! Atomic packets, local proof links and distinct per-record memberships/clocks.
use kmp_adapter_embedded::{EmbeddedKernelStore, verify_bundle};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:batch-check";
const AT: &str = "2026-09-01T10:00:00Z";

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":arguments}});
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    let response: Value = serde_json::from_str(&response).expect("JSON");
    assert!(response.get("error").is_none(), "{response}");
    response["result"].clone()
}

async fn events(store: &EmbeddedKernelStore) -> u64 {
    verify_bundle(&store.export_bundle().await.expect("export"))
        .expect("verified bundle")
        .event_count
}

fn packet() -> Value {
    json!({"about":ABOUT,"actor":"source-reader","observed_at":AT,
    "idempotency_key":"batch:configuration","labels":{"component":["neb"]},
    "memories":[
        {"id":"choice","kind":"decision","summary":"The team chooses a retry after token refresh.",
         "evidence":"Decision R2 selects the retry because request logs show a refresh race.",
         "observed_at":"2026-09-01T09:50:00Z","occurred_at":"2026-09-01T09:45:00Z",
         "connect_to":[{"ref":"@logs","rel":"chosen_because","class":"causal",
         "why":"A retry addresses the refresh race shown by the request logs.",
         "evidence":"R2 cites the R1 refresh race as the reason to retry."}]},
        {"id":"logs","kind":"observation","summary":"Request logs show a token refresh race.",
         "evidence":"R1 shows refresh success followed immediately by an unauthorized request; its registry lists Nebula cache and NC for neb.",
         "labels":{"alias":["Nebula cache","NC"],"component":["neb"]},
         "observed_at":"2026-09-01T09:35:00Z"}
    ]})
}

#[tokio::test]
async fn forward_local_links_commit_once_with_complete_labels_proof_and_clocks() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let mut args = packet();
    args["options"] = json!({"dry_run":true});
    let preview = call(&server, "kmp_write_memory", args.clone()).await;
    assert_eq!(preview["isError"], false, "{preview}");
    let planned = &preview["structuredContent"];
    assert_eq!(planned["accepted"], false);
    assert_eq!(planned["validation"]["scope"], "current_store");
    assert_eq!(
        planned["relation_quality"][0]["prior_context_sources"],
        json!(["current_request"])
    );
    assert_eq!(events(&store).await, 0);
    args["options"]["dry_run"] = json!(false);
    let written = call(&server, "kmp_write_memory", args.clone()).await;
    assert_eq!(written["structuredContent"]["accepted"], true, "{written}");
    assert_eq!(events(&store).await, 1, "the packet is one event");
    let refs = &written["structuredContent"]["local_refs"];
    assert_eq!(refs, &planned["local_refs"]);
    let replay = call(&server, "kmp_write_memory", args).await;
    assert_eq!(replay["structuredContent"]["accepted"], true, "{replay}");
    assert_eq!(events(&store).await, 1);
    for (id, count, observed, occurred) in [
        (
            "choice",
            1,
            "2026-09-01T09:50:00Z",
            Some("2026-09-01T09:45:00Z"),
        ),
        ("logs", 3, "2026-09-01T09:35:00Z", None),
    ] {
        let inspected = call(
            &server,
            "kmp_inspect",
            json!({"about":ABOUT,"ref":refs[id],"include":{"raw":true}}),
        )
        .await;
        assert_eq!(inspected["isError"], false, "{inspected}");
        let record = inspected["structuredContent"]["raw"]
            .as_array()
            .expect("raw")
            .iter()
            .find(|x| x["ref"] == refs[id])
            .expect("record");
        let coords = record["coordinates"].as_array().expect("coordinates");
        assert_eq!(coords.len(), count);
        for coord in coords {
            assert_eq!(coord["observed_at"], observed);
            assert_eq!(coord["occurred_at"].as_str(), occurred);
            assert!(coord["ingested_at"].is_string());
        }
        assert_eq!(
            record["text"],
            packet()["memories"][if id == "choice" { 0 } else { 1 }]["summary"]
        );
    }
    let trace = call(
        &server,
        "kmp_trace",
        json!({"about":ABOUT,"from":refs["choice"],"to":refs["logs"]}),
    )
    .await;
    assert_eq!(trace["isError"], false, "{trace}");
    assert!(
        trace.to_string().contains("R2 cites the R1 refresh race"),
        "{trace}"
    );
    // A different logical write cannot overwrite entries with identical wording.
    let mut distinct = packet();
    distinct["idempotency_key"] = json!("batch:next-observation");
    let next = call(&server, "kmp_write_memory", distinct).await;
    assert_eq!(next["structuredContent"]["accepted"], true, "{next}");
    assert_ne!(next["structuredContent"]["local_refs"], *refs);
    assert_eq!(events(&store).await, 2);
}

#[tokio::test]
async fn invalid_member_or_local_name_leaves_the_entire_packet_unwritten() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let mut cases = Vec::new();
    let mut duplicate = packet();
    duplicate["memories"][1]["id"] = json!("choice");
    cases.push(duplicate);
    let mut missing = packet();
    missing["memories"][0]["connect_to"][0]["ref"] = json!("@missing");
    cases.push(missing);
    let mut proof = packet();
    proof["memories"][1]
        .as_object_mut()
        .expect("record object")
        .remove("evidence");
    cases.push(proof);
    let mut labels = packet();
    labels
        .as_object_mut()
        .expect("packet object")
        .remove("labels");
    cases.push(labels);
    let mut bad_ref = packet();
    bad_ref["memories"][1]["ref"] = json!("project:other:entry:logs");
    cases.push(bad_ref);
    let mut collision = packet();
    for m in collision["memories"]
        .as_array_mut()
        .expect("packet records")
    {
        m["ref"] = json!("project:batch-check:entry:collision");
    }
    cases.push(collision);
    let mut self_link = packet();
    self_link["memories"][0]["connect_to"][0]["ref"] = json!("@choice");
    cases.push(self_link);
    for args in cases {
        let rejected = call(&server, "kmp_write_memory", args).await;
        assert_eq!(rejected["isError"], true, "{rejected}");
        assert_eq!(
            rejected["structuredContent"]["error"]["code"],
            "invalid_argument"
        );
        assert!(rejected.to_string().contains("memories["), "{rejected}");
        assert_eq!(events(&store).await, 0);
    }
}

#[tokio::test]
async fn missing_stored_target_fails_kernel_validation_with_no_partial_commit() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let mut args = packet();
    let absent = "project:batch-check:entry:absent";
    args["memories"][0]["connect_to"][0]["ref"] = json!(absent);
    args["read_context"] = json!({"inspected_refs":[absent]});
    for dry_run in [true, false] {
        args["options"] = json!({"dry_run":dry_run});
        let rejected = call(&server, "kmp_write_memory", args.clone()).await;
        assert_eq!(rejected["isError"], true, "{rejected}");
        assert_eq!(events(&store).await, 0);
    }
}
