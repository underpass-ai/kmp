use kmp_adapter_embedded::{EmbeddedKernelStore, verify_bundle};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

fn store_dir() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&root).expect("scratch root");
    tempfile::Builder::new()
        .prefix("writer-input-types-")
        .tempdir_in(root)
        .expect("isolated store")
}

fn packet() -> Value {
    json!({
        "about":"project:writer-input-types", "actor":"writer",
        "observed_at":"2026-09-01T09:00:00Z", "idempotency_key":"input-type-repair-v1",
        "labels":{"component":["cache"]},
        "memories":[{"id":"logs", "kind":"observation",
            "summary":"The cache returns a hit for the repeated request.",
            "evidence":"R1 records a cache hit for the repeated request."}]
    })
}

async fn call(server: &KernelMcpServer, arguments: Value) -> Value {
    let reply = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0", "id":1, "method":"tools/call",
            "params":{"name":"kmp_write_memory", "arguments":arguments}})
            .to_string(),
        )
        .await
        .expect("MCP reply");
    serde_json::from_str::<Value>(&reply).expect("JSON reply")["result"].clone()
}

async fn assert_no_write(store: &EmbeddedKernelStore) {
    assert_eq!(
        verify_bundle(&store.export_bundle().await.expect("export"))
            .expect("valid bundle")
            .event_count,
        0
    );
}

#[tokio::test]
async fn wrong_collection_types_are_explicit_and_strings_are_never_coerced() {
    let dir = store_dir();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    for (value, received) in [
        (Value::Null, "null"),
        (json!({}), "object"),
        (json!(true), "boolean"),
        (json!(12), "number"),
        (json!("[]"), "string"),
        (json!(packet()["memories"].to_string()), "string"),
    ] {
        let mut args = packet();
        args["memories"] = value;
        let result = call(&server, args).await;
        assert_eq!(result["isError"], true, "{result}");
        let feedback = &result["structuredContent"]["feedback"][0];
        assert_eq!(feedback["code"], "INVALID_TYPE", "{result}");
        assert_eq!(feedback["field"], "memories");
        assert_eq!(feedback["expected_type"], "array");
        assert_eq!(feedback["received_type"], received);
        assert!(feedback["action"].is_null());
        assert_no_write(&store).await;
    }
    let repaired = call(&server, packet()).await;
    assert_eq!(
        repaired["structuredContent"]["accepted"], true,
        "{repaired}"
    );
}

#[tokio::test]
async fn absent_operation_and_empty_collection_are_distinct_from_wrong_type() {
    let dir = store_dir();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let mut absent = packet();
    absent.as_object_mut().expect("packet").remove("memories");
    let result = call(&server, absent).await;
    assert_eq!(
        result["structuredContent"]["feedback"][0]["code"],
        "WRITE_OPERATION_REQUIRED"
    );
    let mut empty = packet();
    empty["memories"] = json!([]);
    let result = call(&server, empty).await;
    let feedback = &result["structuredContent"]["feedback"][0];
    assert_eq!(feedback["code"], "EMPTY_MEMORIES");
    assert_eq!(feedback["field"], "memories");
    assert!(feedback.get("expected_type").is_none());
    assert!(feedback.get("received_type").is_none());
    assert_no_write(&store).await;
}

#[tokio::test]
async fn wrong_member_type_locates_the_member_and_keeps_the_valid_prefix_uncommitted() {
    let dir = store_dir();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    for (value, received) in [
        (json!("{}"), "string"),
        (Value::Null, "null"),
        (json!([]), "array"),
        (json!(1), "number"),
        (json!(false), "boolean"),
    ] {
        let mut args = packet();
        args["memories"]
            .as_array_mut()
            .expect("records")
            .push(value);
        let result = call(&server, args).await;
        let feedback = &result["structuredContent"]["feedback"][0];
        assert_eq!(result["isError"], true, "{result}");
        assert_eq!(feedback["code"], "INVALID_TYPE");
        assert_eq!(feedback["field"], "memories[1]");
        assert_eq!(feedback["expected_type"], "object");
        assert_eq!(feedback["received_type"], received);
        assert_no_write(&store).await;
    }
}
