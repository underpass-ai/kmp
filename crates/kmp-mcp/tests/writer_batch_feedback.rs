use kmp_adapter_embedded::{EmbeddedKernelStore, verify_bundle};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

fn store_dir() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&root).expect("scratch root");
    tempfile::Builder::new()
        .prefix("writer-batch-feedback-")
        .tempdir_in(root)
        .expect("isolated store")
}

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let reply = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0", "id":1, "method":"tools/call",
                "params":{"name":tool,"arguments":arguments}})
            .to_string(),
        )
        .await
        .expect("MCP reply");
    serde_json::from_str::<Value>(&reply).expect("JSON reply")["result"].clone()
}

// These authored fixtures have justified links; exercise the explicit review round.
async fn acknowledge(server: &KernelMcpServer, pending: Value) -> Value {
    assert_eq!(
        pending["structuredContent"]["status"], "needs_review",
        "{pending}"
    );
    let action = &pending["structuredContent"]["next_actions"][0];
    call(server, "kmp_write_memory", action["arguments"].clone()).await
}

fn packet() -> Value {
    json!({
        "about":"project:batch-feedback", "actor":"writer",
        "observed_at":"2026-09-01T09:00:00Z", "idempotency_key":"packet-v1",
        "labels":{"component":["cache"]},
        "memories":[
            {"id":"good","kind":"observation","summary":"The cache logs show a failed request.","evidence":"R1 records a failed cache request."},
            {"id":"choice","kind":"decision","summary":"The team chooses a cache retry."},
            {"id":"policy","kind":"constraint","summary":"Los datos de clientes deben permanecer en la UE.","summary_en":"Customer data must remain in the EU.","evidence":"P1 requires customer data to remain in the UE."},
            {"id":"reason","kind":"observation","summary":"The cache retry addresses the failed request.","evidence":"D1 cites the failed request as its reason.",
                "connect_to":[{"ref":"@good","rel":"chosen_because","class":"motivational","why":"The failed request motivates the retry.","evidence":""}]}
        ]
    })
}

#[tokio::test]
async fn independent_record_errors_arrive_together_and_repair_remains_atomic() {
    let dir = store_dir();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let mut args = packet();
    let rejected = call(&server, "kmp_write_memory", args.clone()).await;
    assert_eq!(rejected["isError"], true, "{rejected}");
    let feedback = rejected["structuredContent"]["feedback"]
        .as_array()
        .expect("feedback");
    assert_eq!(feedback.len(), 3, "{rejected}");
    for (item, (code, field)) in feedback.iter().zip([
        ("MEMORY_EVIDENCE_REQUIRED", "memories[1].evidence"),
        ("INVALID_SEARCH_SUMMARY", "memories[2].summary_en"),
        (
            "RELATION_PROOF_REQUIRED",
            "memories[3].connect_to[0].evidence",
        ),
    ]) {
        assert_eq!(item["code"], code);
        assert_eq!(item["field"], field);
        assert!(item["action"].is_null(), "never invent a repair");
    }
    let empty = store.export_bundle().await.expect("empty bundle");
    assert_eq!(verify_bundle(&empty).expect("valid bundle").event_count, 0);

    args["memories"][1]["evidence"] = json!("D1 records the team's cache retry decision.");
    args["memories"][2]["summary_en"] = json!("Customer data must remain in the UE (EU).");
    args["memories"][3]["connect_to"][0]["evidence"] =
        json!("D1 cites the failed request in R1 as the reason for cache retry.");
    args["options"] = json!({"dry_run":true});
    let preview = call(&server, "kmp_write_memory", args.clone()).await;
    assert_eq!(preview["isError"], false, "{preview}");
    assert_eq!(preview["structuredContent"]["dry_run"], true);
    assert_eq!(store.export_bundle().await.expect("still empty"), empty);
    args["options"]["dry_run"] = json!(false);
    let pending = call(&server, "kmp_write_memory", args).await;
    let accepted = acknowledge(&server, pending).await;
    assert_eq!(
        accepted["structuredContent"]["accepted"], true,
        "{accepted}"
    );
    assert_eq!(accepted["structuredContent"]["coverage"]["memories"], 4);
}

#[tokio::test]
async fn grouped_refusals_then_context_review_preserve_local_reference_paths() {
    let dir = store_dir();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let mut seed = packet();
    seed["idempotency_key"] = json!("seed");
    seed["memories"] = json!([seed["memories"][0]]);
    let original = call(&server, "kmp_write_memory", seed).await;
    let target = &original["structuredContent"]["local_refs"]["good"];
    assert!(target.is_string(), "{original}");
    let before = store.export_bundle().await.expect("seed bundle");
    let mut request = packet();
    request["memories"] = json!([
        {"id":"missing","kind":"observation","summary":"The cache retry addresses the failed request.","evidence":"D1 records the retry rationale.",
            "connect_to":[{"ref":"@absent","rel":"chosen_because","class":"motivational","why":"The failed request motivates the retry.","evidence":"D1 cites R1."}]},
        {"id":"unread","kind":"decision","summary":"The team chooses a cache retry.","evidence":"",
            "connect_to":[{"ref":target,"rel":"chosen_because","class":"motivational","why":"The failed request motivates the retry.","evidence":"D1 cites R1."}]}
    ]);
    let rejected = call(&server, "kmp_write_memory", request.clone()).await;
    let feedback = &rejected["structuredContent"]["feedback"];
    assert_eq!(feedback.as_array().expect("feedback").len(), 2);
    assert_eq!(feedback[0]["code"], "UNKNOWN_LOCAL_REF");
    assert_eq!(feedback[0]["field"], "memories[0].connect_to[0].ref");
    assert_eq!(feedback[1]["code"], "MEMORY_EVIDENCE_REQUIRED");
    assert_eq!(feedback[1]["field"], "memories[1].evidence");
    assert_eq!(store.export_bundle().await.expect("unchanged"), before);
    assert!(feedback[1]["action"].is_null());
    request["memories"][1]["evidence"] = json!("D1 records the retry decision.");
    request["memories"][0]["connect_to"][0]["ref"] = target.clone();
    request["read_context"] = json!({"inspected_refs":[target]});
    let pending = call(&server, "kmp_write_memory", request).await;
    let repaired = acknowledge(&server, pending).await;
    assert_eq!(
        repaired["structuredContent"]["accepted"], true,
        "{repaired}"
    );
}

#[tokio::test]
async fn summary_attachment_collects_repairs_and_preserves_all_stored_facts() {
    let dir = store_dir();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let mut original = packet();
    original["memories"] = json!([
        {"id":"a","kind":"observation","summary":"The cache uses release v1.0.","evidence":"R1 identifies the cache release."},
        {"id":"b","kind":"observation","summary":"The gateway uses release v2.0.","evidence":"R2 identifies the gateway release."},
        {"id":"c","kind":"observation","summary":"The worker uses release v3.0.","evidence":"R3 identifies the worker release."}
    ]);
    let accepted = call(&server, "kmp_write_memory", original).await;
    assert_eq!(
        accepted["structuredContent"]["accepted"], true,
        "{accepted}"
    );
    let refs = &accepted["structuredContent"]["local_refs"];
    let mut snapshot = Vec::new();
    for key in ["a", "b", "c"] {
        snapshot.push(
            call(
                &server,
                "kmp_inspect",
                json!({
                    "about":"project:batch-feedback","ref":refs[key],"budget":{"max_bytes":100000}
                }),
            )
            .await,
        );
    }
    let before = store.export_bundle().await.expect("original bundle");
    let mut request = json!({
        "about":"project:batch-feedback","actor":"writer",
        "observed_at":"2026-09-02T09:00:00Z","idempotency_key":"summary-repair-v1",
        "search_summaries":[
            {"ref":refs["a"],"summary_en":"Release v1.0 runs the cache."},
            {"ref":refs["b"],"summary_en":"The gateway runs a release."},
            {"ref":refs["c"],"summary_en":"The worker runs a release."}
        ]
    });
    let rejected = call(&server, "kmp_write_memory", request.clone()).await;
    assert_eq!(rejected["isError"], true, "{rejected}");
    let feedback = rejected["structuredContent"]["feedback"]
        .as_array()
        .expect("feedback");
    assert_eq!(feedback.len(), 2, "{rejected}");
    for (item, field) in feedback.iter().zip([
        "search_summaries[1].summary_en",
        "search_summaries[2].summary_en",
    ]) {
        assert_eq!(item["field"], field);
        assert_eq!(item["code"], "INVALID_SEARCH_SUMMARY");
    }
    assert_eq!(
        store
            .export_bundle()
            .await
            .expect("no attachment committed"),
        before
    );
    request["search_summaries"][1]["summary_en"] = json!("Release v2.0 runs the gateway.");
    request["search_summaries"][2]["summary_en"] = json!("Release v3.0 runs the worker.");
    let repaired = call(&server, "kmp_write_memory", request.clone()).await;
    assert_eq!(
        repaired["structuredContent"]["accepted"], true,
        "{repaired}"
    );
    for (index, key) in ["a", "b", "c"].into_iter().enumerate() {
        let inspected = call(
            &server,
            "kmp_inspect",
            json!({
                "about":"project:batch-feedback","ref":refs[key],"budget":{"max_bytes":100000}
            }),
        )
        .await;
        let body = &inspected["structuredContent"];
        let prior = &snapshot[index]["structuredContent"];
        assert_eq!(body["object"]["text"], prior["object"]["text"]);
        assert_eq!(body["object"]["kind"], prior["object"]["kind"]);
        assert_eq!(body["evidence"], prior["evidence"]);
        assert_eq!(body["links"], prior["links"]);
        assert_eq!(
            body["object"]["metadata"]["summary_en"],
            request["search_summaries"][index]["summary_en"]
        );
    }
}
