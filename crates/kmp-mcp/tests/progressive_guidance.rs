//! Refusal -> guide topic -> source-backed correction, through native MCP.
#[path = "support/reviewed_writer.rs"]
mod reviewed_writer;
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
    reviewed_writer::review_authored_write(
        server,
        serde_json::from_str::<Value>(&reply).expect("JSON")["result"].clone(),
    )
    .await
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

/// Reads the one pointer line a refusal carries and opens that topic: the
/// card must exist and explain the refused tool's arguments.
async fn read_help(server: &KernelMcpServer, tool: &str, result: &Value) -> String {
    assert!(result["structuredContent"].get("help").is_none());
    let text = result["content"][0]["text"].as_str().expect("fallback");
    let line = text
        .lines()
        .find_map(|line| line.strip_prefix("Help: kmp_guide "))
        .unwrap_or_else(|| panic!("{tool}: no help pointer in {text}"));
    let arguments: Value = serde_json::from_str(line).expect("pointer arguments");
    let card = call(server, "kmp_guide", arguments.clone()).await;
    assert_eq!(card["isError"], false, "{card}");
    let body = &card["structuredContent"];
    assert_eq!(body["topic"], arguments["topic"]);
    assert!(!body["text"].as_str().expect("card text").is_empty());
    assert!(body["parameters"][tool].is_array(), "{tool}: {body}");
    arguments["topic"].as_str().expect("topic").to_string()
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
        let name = tool["name"].as_str().expect("name");
        let rejected = call(&server, name, json!({"misspelled_argument":true})).await;
        assert_eq!(rejected["isError"], true, "{rejected}");
        if name == "kmp_guide" {
            // The guide does not point at itself.
            let text = rejected["content"][0]["text"].as_str().expect("text");
            assert!(!text.contains("Help:"), "{text}");
            continue;
        }
        read_help(&server, name, &rejected).await;
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
        read_help(&server, "kmp_write_memory", &refused).await,
        "relations"
    );
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
    let text = result["content"][0]["text"].as_str().expect("text");
    assert!(!text.contains("Help:"), "{text}");
    assert_eq!(store.export_bundle().await.expect("after"), before);
}
