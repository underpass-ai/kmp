#![allow(dead_code)]
use kmp_adapter_embedded::{EmbeddedKernelStore, verify_bundle};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

pub const ABOUT: &str = "project:write-neighborhood";

pub fn scratch() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&root).expect("scratch");
    tempfile::tempdir_in(root).expect("isolated store")
}

pub async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":arguments}});
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    let result = serde_json::from_str::<Value>(&response).expect("JSON")["result"].clone();
    assert_eq!(result["isError"], false, "{result}");
    result["structuredContent"].clone()
}

pub fn packet() -> Value {
    json!({"about":ABOUT,"actor":"writer","idempotency_key":"review-packet","labels":{"task":["copy"]},
    "memories":[
        {"id":"limit","kind":"constraint","summary":"P9 limits the copy to 80 MB.","evidence":"P9: maximum 80 MB."},
        {"id":"run","kind":"observation","summary":"R4 records a 74 MB copy.","evidence":"R4: 74 MB.",
         "connect_to":[{"ref":"limit","rel":"satisfies_constraint","class":"constraint",
             "why":"The recorded 74 MB is below P9's 80 MB maximum.","evidence":"R4: 74 MB; P9: maximum 80 MB."}]}
    ]})
}

pub async fn resume(server: &KernelMcpServer, pending: &Value) -> Value {
    assert_eq!(pending["status"], "needs_review", "{pending}");
    let action = &pending["next_actions"][0];
    call(
        server,
        action["tool"].as_str().expect("verb"),
        action["arguments"].clone(),
    )
    .await
}

pub async fn seed(
    server: &KernelMcpServer,
    key: &str,
    kind: &str,
    text: &str,
    label: &str,
) -> Value {
    let result = call(server, "kmp_write_memory", json!({"about":ABOUT,"actor":"writer","idempotency_key":key,
        "labels":{"task":[label]},"memories":[{"id":"source","kind":kind,"summary":text,"evidence":text}]})).await;
    assert_eq!(result["status"], "committed", "{result}");
    result["local_refs"]["source"].clone()
}

pub async fn events(path: &std::path::Path) -> usize {
    let store = EmbeddedKernelStore::open(path).expect("store");
    verify_bundle(&store.export_bundle().await.expect("export"))
        .expect("bundle")
        .event_count as usize
}
