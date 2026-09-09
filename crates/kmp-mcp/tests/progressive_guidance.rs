//! Refusal -> targeted lesson -> source-backed correction, through native MCP.
use kmp_adapter_embedded::{EmbeddedKernelStore, verify_bundle};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}});
    let reply = server
        .handle_json_line(&request.to_string())
        .await
        .expect("reply");
    serde_json::from_str::<Value>(&reply).expect("JSON")["result"].clone()
}

async fn guide(server: &KernelMcpServer) {
    let requests: Vec<Value> = serde_json::from_str(include_str!(
        "../../../plugins/kmp/guide/guide.requests.json"
    ))
    .expect("guide requests");
    for request in requests {
        let result = call(server, "kmp_ingest", request).await;
        assert_eq!(result["isError"], false, "{result}");
    }
}

async fn read_help(server: &KernelMcpServer, result: &Value) {
    let help = &result["structuredContent"]["help"];
    let text = result["content"][0]["text"].as_str().expect("fallback");
    for action in
        std::iter::once(&help["guide"]).chain(help["examples"].as_array().expect("lessons"))
    {
        let tool = action["tool"].as_str().expect("tool");
        let args = action["arguments"].clone();
        assert!(text.contains(args["ref"].as_str().expect("ref")));
        let lesson = call(server, tool, args.clone()).await;
        assert_eq!(lesson["isError"], false, "{lesson}");
        assert_eq!(lesson["structuredContent"]["object"]["ref"], args["ref"]);
        assert!(
            !lesson["structuredContent"]["object"]["text"]
                .as_str()
                .expect("lesson body")
                .is_empty()
        );
        let mut page = lesson["structuredContent"].clone();
        let mut pages = 1;
        while page["page"]["has_more"] == true {
            pages += 1;
            assert!(pages <= 32, "lesson continuation must finish: {page}");
            let continuation = &page["next_actions"][0];
            assert_eq!(continuation["arguments"]["ref"], args["ref"]);
            assert_eq!(continuation["arguments"]["about"], args["about"]);
            let next = call(
                server,
                continuation["tool"].as_str().expect("continuation tool"),
                continuation["arguments"].clone(),
            )
            .await;
            assert_eq!(next["isError"], false, "{next}");
            page = next["structuredContent"].clone();
            assert!(page["page"]["returned"].as_u64().expect("progress") > 0);
        }
        assert_eq!(page["page"]["has_more"], false);
    }
}

#[tokio::test]
async fn every_public_verb_offers_executable_help_after_a_shape_refusal() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    guide(&server).await;
    let list = server
        .handle_json_line(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
        .await
        .expect("tools");
    let list: Value = serde_json::from_str(&list).expect("JSON");
    for tool in list["result"]["tools"].as_array().expect("tools") {
        let rejected = call(
            &server,
            tool["name"].as_str().expect("name"),
            json!({"misspelled_argument":true}),
        )
        .await;
        assert_eq!(rejected["isError"], true, "{rejected}");
        read_help(&server, &rejected).await;
    }
}

#[tokio::test]
async fn targeted_lessons_preserve_atomic_refusal_and_do_not_supply_missing_proof() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    guide(&server).await;
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let before = store.export_bundle().await.expect("before");
    let mut packet = json!({
        "about":"project:progressive-help", "actor":"writer",
        "idempotency_key":"source-1", "observed_at":"2026-09-01T09:00:00Z",
        "labels":{"component":["cache"]},
        "memories":[
            {"id":"source","kind":"observation","summary":"The cache failed.",
             "evidence":"Report R1: the cache failed."},
            {"id":"choice","kind":"decision","summary":"Retry the cache.",
             "evidence":"Decision D1: retry because R1 recorded a cache failure.",
             "connect_to":[{"ref":"@source","rel":"chosen_because","class":"causal",
                "why":"The cache failure prompted the retry.", "evidence":""}]}
        ]
    });
    let refused = call(&server, "kmp_write_memory", packet.clone()).await;
    assert_eq!(refused["isError"], true);
    let feedback = &refused["structuredContent"]["feedback"][0];
    assert_eq!(feedback["code"], "RELATION_PROOF_REQUIRED");
    assert!(feedback["action"].is_null(), "no invented evidence");
    assert_eq!(
        refused["structuredContent"]["help"]["examples"][0]["arguments"]["ref"],
        "guide:kmp-agent:example:decision-history"
    );
    read_help(&server, &refused).await;
    assert_eq!(
        store
            .export_bundle()
            .await
            .expect("after refusal and lesson"),
        before
    );
    packet["memories"][1]["connect_to"][0]["evidence"] =
        json!("D1: retry because R1 recorded a cache failure.");
    let accepted = call(&server, "kmp_write_memory", packet).await;
    assert_eq!(
        accepted["structuredContent"]["accepted"], true,
        "{accepted}"
    );
    assert!(accepted["structuredContent"].get("help").is_none());
    let after = store.export_bundle().await.expect("after");
    assert_eq!(
        verify_bundle(&after).expect("verified").event_count,
        verify_bundle(&before).expect("verified").event_count + 1
    );
}

#[tokio::test]
async fn failed_guide_reads_do_not_create_a_recursive_read_or_write() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let before = store.export_bundle().await.expect("before");
    let result = call(
        &server,
        "kmp_inspect",
        json!({"about":"guide:kmp-agent",
        "ref":"guide:kmp-agent:verb:write"}),
    )
    .await;
    assert_eq!(result["isError"], true);
    assert!(result["structuredContent"].get("help").is_none());
    assert_eq!(store.export_bundle().await.expect("after"), before);
}
