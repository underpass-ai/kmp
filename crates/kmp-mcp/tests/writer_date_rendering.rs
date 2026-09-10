use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

#[tokio::test]
async fn strict_writer_accepts_iso_date_without_rewriting_source_or_summary() {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&scratch).expect("scratch root");
    let dir = tempfile::tempdir_in(scratch).expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut arguments = json!({
        "about":"project:iso-date-repro", "actor":"reviewer",
        "observed_at":"2026-08-17T08:20:00Z", "labels":{"source":["B01"]},
        "memories":[{
            "id":"one", "kind":"observation",
            "summary":"El 17 de agosto de 2026 se abrió la ruta.",
            "summary_en":"The route opened on 2026-09-17.",
            "evidence":"B01: El 17 de agosto de 2026 se abrió la ruta."
        }],
        "options":{"dry_run":true}
    });
    let rejected = call(&server, arguments.clone()).await;
    assert_eq!(rejected["isError"], true, "{rejected}");
    assert_eq!(
        rejected["structuredContent"]["feedback"][0]["code"],
        "INVALID_SEARCH_SUMMARY"
    );
    arguments["memories"][0]["summary_en"] = json!("The route opened on 2026-08-17.");
    let accepted = call(&server, arguments).await;
    assert_eq!(accepted["isError"], false, "{accepted}");
    assert_eq!(accepted["structuredContent"]["dry_run"], true);
    assert_eq!(accepted["structuredContent"]["accepted"], false);
    let preview = accepted["structuredContent"]["ingest_preview"].to_string();
    assert!(
        preview.contains("El 17 de agosto de 2026 se abrió la ruta."),
        "{preview}"
    );
    assert!(
        preview.contains("The route opened on 2026-08-17."),
        "{preview}"
    );
}

async fn call(server: &KernelMcpServer, arguments: Value) -> Value {
    let reply = server
        .handle_json_line(
            &json!({
                "jsonrpc":"2.0", "id":1, "method":"tools/call",
                "params":{"name":"kmp_write_memory","arguments":arguments}
            })
            .to_string(),
        )
        .await
        .expect("MCP reply");
    serde_json::from_str::<Value>(&reply).expect("JSON reply")["result"].clone()
}
