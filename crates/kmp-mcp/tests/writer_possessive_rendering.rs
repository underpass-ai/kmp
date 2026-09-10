use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

#[tokio::test]
async fn strict_writer_accepts_possessives_but_rejects_replaced_identifiers() {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&scratch).expect("scratch root");
    let dir = tempfile::tempdir_in(scratch).expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let source = "R7 utiliza el plan O2.";
    let evidence = "S3: R7 utiliza el plan O2.";
    for (rendering, valid) in [
        ("R7 uses the plan of O2.", true),
        ("R7 uses O2's plan.", true),
        ("R7 uses O2’s plan.", true),
        ("R7 uses O3's plan.", false),
        ("R7 uses O3’s plan.", false),
        ("R7 uses O20's plan.", false),
    ] {
        let request = json!({"jsonrpc":"2.0", "id":1, "method":"tools/call",
        "params":{"name":"kmp_write_memory", "arguments":{
            "about":"project:possessive-repro", "actor":"reproduction",
            "observed_at":"2026-09-08T10:00:00Z", "labels":{"task":["possessive-repro"]},
            "memories":[{"id":"source", "kind":"observation", "summary":source,
                "summary_en":rendering, "evidence":evidence}],
            "options":{"dry_run":true}
        }}});
        let reply = server
            .handle_json_line(&request.to_string())
            .await
            .expect("MCP reply");
        let reply: Value = serde_json::from_str(&reply).expect("JSON");
        let result = &reply["result"];
        assert_eq!(result["isError"], !valid, "{rendering}: {result}");
        let body = &result["structuredContent"];
        if valid {
            assert_eq!(body["accepted"], false);
            assert_eq!(body["status"], "validated");
            let memory = &body["ingest_preview"]["memory"];
            assert_eq!(memory["entries"][0]["text"], source);
            assert_eq!(memory["entries"][0]["metadata"]["summary_en"], rendering);
            assert_eq!(memory["evidence"][0]["text"], evidence);
        } else {
            assert_eq!(body["feedback"][0]["code"], "INVALID_SEARCH_SUMMARY");
            assert!(
                body["feedback"][0]["reason"]
                    .as_str()
                    .expect("typed feedback reason")
                    .contains("o2")
            );
        }
    }
}
