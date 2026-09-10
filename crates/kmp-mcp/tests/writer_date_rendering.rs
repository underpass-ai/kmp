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

#[tokio::test]
async fn strict_writer_carries_partial_dates_without_padding_or_rewriting_evidence() {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&scratch).expect("scratch root");
    let dir = tempfile::tempdir_in(scratch).expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let source = "El servicio de caché de producción falló el 16 de agosto de 2026 a las 23:55 UTC. El equipo recibió este parte el 18 de agosto a las 11:40 UTC. Entorno: producción. Componentes afectados: caché y pasarela. El parte no informa de una caída de staging.";
    let rendering = "The production cache service failed on 2026-08-16 at 23:55 UTC; the team received INC-42 on 2026-08-18 at 11:40 UTC. Production cache and gateway were affected, with no staging outage reported.";
    let arguments = json!({
        "about":"project:partial-date-repro", "actor":"reviewer",
        "observed_at":"2026-08-18T11:40:00Z", "labels":{"source":["B02"]},
        "memories":[{
            "id":"b02", "kind":"observation", "occurred_at":"2026-08-16T23:55:00Z",
            "summary":source, "summary_en":rendering, "evidence":source
        }],
        "options":{"dry_run":true}
    });
    for (date, valid) in [
        ("2026-08-18", true),
        ("2026-09-18", false),
        ("2026-08-19", false),
    ] {
        let mut request = arguments.clone();
        request["memories"][0]["summary_en"] = json!(rendering.replace("2026-08-18", date));
        let result = call(&server, request).await;
        assert_eq!(result["isError"], !valid, "{result}");
        if valid {
            let memory = &result["structuredContent"]["ingest_preview"]["memory"];
            assert_eq!(memory["entries"][0]["text"], source);
            assert_eq!(memory["entries"][0]["metadata"]["summary_en"], rendering);
            assert_eq!(memory["evidence"][0]["text"], source);
            assert_eq!(result["structuredContent"]["accepted"], false);
        } else {
            assert_eq!(
                result["structuredContent"]["feedback"][0]["code"],
                "INVALID_SEARCH_SUMMARY"
            );
            assert!(
                result["structuredContent"]["feedback"][0]["reason"]
                    .as_str()
                    .expect("typed feedback reason")
                    .contains("--08-18")
            );
        }
    }
}
