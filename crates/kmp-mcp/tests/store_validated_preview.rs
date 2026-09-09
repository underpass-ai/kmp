//! A preview is checked against the same store as its eventual write.
use kmp_adapter_embedded::{EmbeddedKernelStore, verify_bundle};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:preview";
const SEED: &str = "project:preview:observation:seed";

async fn call(server: &KernelMcpServer, tool: &str, args: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":args}});
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    let response: Value = serde_json::from_str(&response).expect("JSON response");
    assert!(response.get("error").is_none(), "{response}");
    response["result"].clone()
}

async fn events(store: &EmbeddedKernelStore) -> u64 {
    let bundle = store.export_bundle().await.expect("export");
    verify_bundle(&bundle).expect("valid bundle").event_count
}

fn observation(key: &str, reference: &str) -> Value {
    json!({"about":ABOUT,"actor":"test-writer","intent":"record_observation",
        "observed_at":"2026-09-01T10:00:00Z","scope":{"process":"review"},
        "current":{"ref":reference,"kind":"observation","summary":"A source records the configuration.",
        "evidence":"The source contains an explicit configuration entry."},
        "idempotency_key":key,"options":{"strict":false}})
}

#[tokio::test]
async fn preview_checks_existing_refs_without_committing_and_can_then_be_written() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    let store = EmbeddedKernelStore::open(directory.path()).expect("store reader");
    let seed = call(
        &server,
        "kmp_write_memory",
        observation("preview:seed", SEED),
    )
    .await;
    assert_eq!(seed["structuredContent"]["accepted"], true, "{seed}");
    let next = "project:preview:observation:next";
    let mut args = observation("preview:next", next);
    args["options"]["dry_run"] = json!(true);
    args["connect_to"] = json!([{"ref":"project:preview:observation:missing",
        "rel":"uses_background","class":"evidential","why":"The prior configuration gives context.",
        "evidence":"The source links this change to the prior configuration.","confidence":"high"}]);
    let invalid = call(&server, "kmp_write_memory", args.clone()).await;
    assert_eq!(invalid["isError"], true, "{invalid}");
    assert_eq!(
        invalid["structuredContent"]["error"]["code"],
        "invalid_argument"
    );
    assert_eq!(events(&store).await, 1);
    args["connect_to"][0]["ref"] = json!(SEED);
    let preview = call(&server, "kmp_write_memory", args.clone()).await;
    assert_eq!(preview["isError"], false, "{preview}");
    assert_eq!(preview["structuredContent"]["accepted"], false);
    assert_eq!(
        preview["structuredContent"]["validation"]["scope"],
        "current_store"
    );
    assert_eq!(events(&store).await, 1);
    let absent = call(&server, "kmp_inspect", json!({"about":ABOUT,"ref":next})).await;
    assert_eq!(absent["structuredContent"]["error"]["code"], "not_found");
    args["options"]["dry_run"] = json!(false);
    let committed = call(&server, "kmp_write_memory", args).await;
    assert_eq!(
        committed["structuredContent"]["accepted"], true,
        "{committed}"
    );
    assert_eq!(events(&store).await, 2);
}

#[tokio::test]
async fn canonical_preview_rejects_an_unresolved_membership_before_any_event() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    let store = EmbeddedKernelStore::open(directory.path()).expect("store reader");
    let args = json!({"about":ABOUT,"idempotency_key":"preview:canonical","dry_run":true,
        "memory":{"dimensions":[{"id":"review","kind":"task"}],
        "entries":[{"id":SEED,"kind":"observation","text":"A source records the configuration.",
        "coordinates":[{"dimension":"task","scope_id":"review","observed_at":"2026-09-01T10:00:00Z"}]}],
        "relations":[{"from":"review","to":"project:preview:observation:missing","rel":"contains_entry","class":"structural"}]}});
    let rejected = call(&server, "kmp_ingest", args).await;
    assert_eq!(rejected["isError"], true, "{rejected}");
    assert_eq!(events(&store).await, 0);
}
