use kmp_adapter_embedded::{EmbeddedKernelStore, verify_bundle};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

fn packet() -> Value {
    json!({
        "about":"project:write-signals", "actor":"writer", "idempotency_key":"one",
        "observed_at":"2026-09-01T10:00:00Z", "labels":{"source":["R1","R2"]},
        "memories":[
            {"id":"a","kind":"observation","summary":"The valve failed overnight.","evidence":"R1 records the failure.","occurred_at":"2026-08-31T23:00:00Z"},
            {"id":"b","kind":"decision","summary":"The team approved replacing the valve.","evidence":"R2 records approval.","valid_from":"2026-09-03T00:00:00Z"}
        ]
    })
}

async fn call(server: &KernelMcpServer, args: Value) -> Value {
    let line = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"kmp_write_memory","arguments":args}})
    .to_string();
    let reply = server.handle_json_line(&line).await.expect("reply");
    serde_json::from_str::<Value>(&reply).expect("JSON")["result"].clone()
}

#[tokio::test]
async fn signals_distinguish_commit_replay_preview_and_rejection() {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&scratch).expect("scratch");
    let dir = tempfile::tempdir_in(&scratch).expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let mut preview = packet();
    preview["options"] = json!({"dry_run":true});
    let preview = call(&server, preview).await;
    assert_eq!(preview["structuredContent"]["status"], "validated");
    assert!(preview["structuredContent"].get("clocks").is_none());
    let mut invalid = packet();
    invalid["memories"][0]["kind"] = json!("missing");
    let rejected = call(&server, invalid).await;
    assert_eq!(rejected["structuredContent"]["status"], "rejected");
    assert_eq!(
        verify_bundle(&store.export_bundle().await.expect("empty"))
            .expect("bundle")
            .event_count,
        0
    );

    let first = call(&server, packet()).await;
    let first = &first["structuredContent"];
    assert_eq!(first["status"], "committed");
    let clocks = &first["clocks"];
    assert_eq!(clocks["scope"], "accepted_command");
    assert_eq!(clocks["entries"], 2);
    assert_eq!(
        clocks["observed"],
        json!({"entries":2,"distinct_values":1,"single_value":"2026-09-01T10:00:00Z"})
    );
    assert_eq!(clocks["occurred"]["entries"], 1);
    assert_eq!(clocks["valid_from"]["entries"], 1);
    assert_eq!(clocks["valid_until"]["entries"], 0);
    assert_eq!(clocks["ingested"]["entries"], 2);
    assert!(
        clocks["ingested"]["single_value"]
            .as_str()
            .expect("ingested")
            .contains('T')
    );
    let replay = call(&server, packet()).await;
    assert_eq!(replay["structuredContent"]["status"], "replayed");
    assert_eq!(&replay["structuredContent"]["clocks"], clocks);
    assert_eq!(
        verify_bundle(&store.export_bundle().await.expect("one write"))
            .expect("bundle")
            .event_count,
        1
    );

    let mut later = packet();
    later["idempotency_key"] = json!("two");
    later["observed_at"] = json!("2026-09-02T10:00:00Z");
    later["memories"][1]["observed_at"] = json!("2026-09-03T10:00:00Z");
    let later = call(&server, later).await;
    assert_eq!(later["structuredContent"]["status"], "committed");
    assert_eq!(
        later["structuredContent"]["clocks"]["observed"],
        json!({"entries":2,"distinct_values":2,"single_value":null})
    );
    let bundle = store.export_bundle().await.expect("export");
    drop(server);
    drop(store);
    let reopened = KernelMcpServer::embedded(dir.path()).expect("restart");
    let replay = call(&reopened, packet()).await;
    assert_eq!(replay["structuredContent"]["status"], "replayed");
    assert_eq!(&replay["structuredContent"]["clocks"], clocks);

    let target_dir = tempfile::tempdir_in(scratch).expect("target");
    let target = EmbeddedKernelStore::open(target_dir.path()).expect("importer");
    target
        .import_bundle(
            &bundle,
            kmp_application::projection_mutations_for_context_event,
        )
        .await
        .expect("import");
    let imported = KernelMcpServer::embedded(target_dir.path()).expect("imported server");
    let replay = call(&imported, packet()).await;
    assert_eq!(replay["structuredContent"]["status"], "replayed");
    assert_eq!(&replay["structuredContent"]["clocks"], clocks);
}
