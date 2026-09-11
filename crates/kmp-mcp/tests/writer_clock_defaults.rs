//! Default clocks are assigned once at ingest, never by the caller or on replay.
use kmp_adapter_embedded::{EmbeddedKernelStore, verify_bundle};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:implicit-observation";

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":arguments}});
    let reply = server
        .handle_json_line(&line.to_string())
        .await
        .expect("reply");
    serde_json::from_str::<Value>(&reply).expect("JSON")["result"].clone()
}

fn packet() -> Value {
    json!({"about":ABOUT,"actor":"writer","idempotency_key":"implicit-clocks",
    "labels":{"component":["cache"],"task":["clock-check"]},
    "memories":[
        {"id":"one","kind":"observation","summary":"The cache failed.","evidence":"R1 reports the cache failure without its event time."},
        {"id":"two","kind":"observation","summary":"The retry succeeded.","evidence":"R2 reports a successful retry after R1, without its event time.","occurred_at":null,
            "connect_to":[{"ref":"one","rel":"follows","class":"procedural","why":"The retry follows the reported failure.","evidence":"R2 reports the retry after R1."}]}
    ]})
}

fn scratch() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&root).expect("scratch");
    tempfile::tempdir_in(root).expect("isolated store")
}

async fn receipt(server: &KernelMcpServer, reply: &Value) -> Value {
    let action = &reply["structuredContent"]["receipt"]["action"];
    let inspected = call(server, "kmp_inspect", action["arguments"].clone()).await;
    assert_eq!(inspected["isError"], false, "{inspected}");
    serde_json::from_str::<Value>(
        inspected["structuredContent"]["object"]["text"]
            .as_str()
            .expect("receipt text"),
    )
    .expect("receipt JSON")["receipt"]
        .clone()
}

#[tokio::test]
async fn omitted_observation_matches_ingestion_and_survives_replay_and_restart() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    let mut preview = packet();
    preview["options"] = json!({"dry_run":true});
    let preview = call(&server, "kmp_write_memory", preview).await;
    assert_eq!(preview["isError"], false, "{preview}");
    assert_eq!(preview["structuredContent"]["status"], "validated");
    assert!(preview["structuredContent"].get("clocks").is_none());
    assert_eq!(preview["structuredContent"]["clock_defaults"]["entries"], 2);
    assert_eq!(
        verify_bundle(&store.export_bundle().await.expect("empty"))
            .expect("bundle")
            .event_count,
        0
    );

    let written = call(&server, "kmp_write_memory", packet()).await;
    assert_eq!(written["isError"], false, "{written}");
    let clocks = &written["structuredContent"]["clocks"];
    assert_eq!(clocks["observed"], clocks["ingested"]);
    assert_eq!(clocks["observed"]["entries"], 2);
    assert_eq!(clocks["observed"]["distinct_values"], 1);
    assert_eq!(clocks["occurred"]["entries"], 0);
    assert_eq!(
        clocks["relations"],
        json!({"relations":1,"occurred":0,"observed":1,
        "ingested":1,"valid_from":0,"valid_until":0})
    );
    let original = receipt(&server, &written).await;
    let at = &original["canonical_memory"]["entries"][0]["coordinates"][0]["ingested_at"];
    assert_eq!(&original["provenance"]["observed_at"], at);
    let link = &original["canonical_memory"]["relations"][0];
    assert_eq!(&link["clocks"]["observed_at"], at);
    assert_eq!(&link["clocks"]["ingested_at"], at);
    assert!(link["clocks"].get("occurred_at").is_none());
    assert!(link.get("coordinate").is_none());
    for entry in original["canonical_memory"]["entries"]
        .as_array()
        .expect("entries")
    {
        for c in entry["coordinates"].as_array().expect("coordinates") {
            assert_eq!(&c["observed_at"], at);
            assert_eq!(&c["ingested_at"], at);
            assert!(c.get("occurred_at").is_none());
        }
    }
    for e in original["canonical_memory"]["evidence"]
        .as_array()
        .expect("evidence")
    {
        assert_eq!(&e["time"], at);
    }
    let replay = call(&server, "kmp_write_memory", packet()).await;
    assert_eq!(replay["structuredContent"]["status"], "replayed");
    assert_eq!(&replay["structuredContent"]["clocks"], clocks);
    assert_eq!(receipt(&server, &replay).await, original);
    drop(server);
    let restarted = KernelMcpServer::embedded(dir.path()).expect("restart");
    let replay = call(&restarted, "kmp_write_memory", packet()).await;
    assert_eq!(replay["structuredContent"]["status"], "replayed");
    assert_eq!(&replay["structuredContent"]["clocks"], clocks);
    assert_eq!(
        verify_bundle(&store.export_bundle().await.expect("one write"))
            .expect("bundle")
            .event_count,
        1
    );
    let bundle = store.export_bundle().await.expect("export");
    let imported_dir = scratch();
    let imported_store = EmbeddedKernelStore::open(imported_dir.path()).expect("import store");
    imported_store
        .import_bundle(
            &bundle,
            kmp_application::projection_mutations_for_context_event,
        )
        .await
        .expect("import");
    let imported = KernelMcpServer::embedded(imported_dir.path()).expect("imported server");
    let replay = call(&imported, "kmp_write_memory", packet()).await;
    assert_eq!(replay["structuredContent"]["status"], "replayed");
    assert_eq!(receipt(&imported, &replay).await, original);

    let mut changed = packet();
    changed["observed_at"] = json!("2026-09-01T10:00:00Z");
    let refused = call(&imported, "kmp_write_memory", changed).await;
    assert_eq!(refused["isError"], true, "{refused}");
    assert_eq!(
        verify_bundle(&imported_store.export_bundle().await.expect("unchanged"))
            .expect("bundle")
            .event_count,
        1
    );
}

#[tokio::test]
async fn record_clocks_do_not_supply_packet_provenance_and_equal_clocks_are_valid() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut args = packet();
    args["observed_at"] = Value::Null;
    args["memories"][0]["observed_at"] = json!("2026-09-01T09:00:00Z");
    args["memories"][0]["occurred_at"] = json!("2026-09-01T09:00:00Z");
    let written = call(&server, "kmp_write_memory", args).await;
    assert_eq!(written["isError"], false, "{written}");
    let stored = receipt(&server, &written).await;
    let first = &stored["canonical_memory"]["entries"][0]["coordinates"][0];
    let second = &stored["canonical_memory"]["entries"][1]["coordinates"][0];
    assert_eq!(first["observed_at"], first["occurred_at"]);
    assert_ne!(first["observed_at"], first["ingested_at"]);
    assert_eq!(second["observed_at"], stored["provenance"]["observed_at"]);
    assert_eq!(second["observed_at"], second["ingested_at"]);
    assert!(second.get("occurred_at").is_none());
    let link = &stored["canonical_memory"]["relations"][0]["clocks"];
    assert_eq!(link["observed_at"], stored["provenance"]["observed_at"]);
    assert_ne!(link["observed_at"], first["observed_at"]);
}

#[tokio::test]
async fn invalid_observation_never_becomes_a_default_or_commits_a_prefix() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    for invalid in [
        json!(""),
        json!(false),
        json!("yesterday"),
        json!("9999-01-01T00:00:00Z"),
    ] {
        let mut args = packet();
        args["memories"][1]["observed_at"] = invalid;
        let rejected = call(&server, "kmp_write_memory", args).await;
        assert_eq!(rejected["isError"], true, "{rejected}");
    }
    let store = EmbeddedKernelStore::open(dir.path()).expect("store");
    assert_eq!(
        verify_bundle(&store.export_bundle().await.expect("empty"))
            .expect("bundle")
            .event_count,
        0
    );
}

#[tokio::test]
async fn explicit_packet_clocks_and_record_null_overrides_are_distinct() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut args = packet();
    args["observed_at"] = json!("2026-09-01T10:00:00Z");
    args["occurred_at"] = json!("2026-09-01T09:00:00Z");
    args["memories"][1]["observed_at"] = Value::Null;
    let written = call(&server, "kmp_write_memory", args).await;
    assert_eq!(written["isError"], false, "{written}");
    let clocks = &written["structuredContent"]["clocks"];
    assert_eq!(clocks["observed"]["distinct_values"], 2);
    assert_eq!(clocks["occurred"]["entries"], 1);
    assert_eq!(clocks["occurred"]["single_value"], "2026-09-01T09:00:00Z");
    let detail = receipt(&server, &written).await;
    let entries = &detail["canonical_memory"]["entries"];
    let link = &detail["canonical_memory"]["relations"][0]["clocks"];
    assert_eq!(link["observed_at"], link["ingested_at"]);
    assert_eq!(
        link["observed_at"],
        entries[1]["coordinates"][0]["observed_at"]
    );
    assert!(link.get("occurred_at").is_none());
    assert!(link.get("valid_from").is_none());
    assert_eq!(
        entries[0]["coordinates"][0]["observed_at"],
        detail["provenance"]["observed_at"]
    );
    assert_ne!(
        entries[0]["coordinates"][0]["observed_at"],
        entries[0]["coordinates"][0]["ingested_at"]
    );
    assert_eq!(
        entries[1]["coordinates"][0]["observed_at"],
        entries[1]["coordinates"][0]["ingested_at"]
    );
    assert!(entries[1]["coordinates"][0].get("occurred_at").is_none());
}

#[tokio::test]
async fn implicit_summary_provenance_does_not_retime_the_source() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut args = packet();
    args["observed_at"] = json!("2026-09-01T10:00:00Z");
    let source = call(&server, "kmp_write_memory", args).await;
    assert_eq!(source["isError"], false, "{source}");
    let old = receipt(&server, &source).await;
    let rendered = call(&server,"kmp_write_memory",json!({"about":ABOUT,"actor":"writer","idempotency_key":"rendering",
        "search_summaries":[{"ref":source["structuredContent"]["local_refs"]["one"],"summary_en":"The cache service stopped working."}]})).await;
    assert_eq!(rendered["isError"], false, "{rendered}");
    let new = receipt(&server, &rendered).await;
    assert_eq!(
        old["canonical_memory"]["entries"][0]["coordinates"],
        new["canonical_memory"]["entries"][0]["coordinates"]
    );
    assert_eq!(
        old["canonical_memory"]["entries"][0]["text"],
        new["canonical_memory"]["entries"][0]["text"]
    );
    assert_ne!(
        old["provenance"]["observed_at"],
        new["provenance"]["observed_at"]
    );
    assert_eq!(
        rendered["structuredContent"]["clock_defaults"],
        json!({"observed_at":"ingested_at","entries":0,"provenance":true})
    );
}

#[tokio::test]
async fn summary_update_keeps_a_canonical_records_unknown_observation() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut args = packet();
    args["options"] = json!({"dry_run":true});
    let preview = call(&server, "kmp_write_memory", args).await;
    let mut canonical = preview["structuredContent"]["ingest_preview"].clone();
    canonical["default_observation_to_ingestion"] = json!(false);
    canonical["dry_run"] = json!(false);
    canonical["provenance"]["observed_at"] = json!("2026-09-01T10:00:00Z");
    let written = call(&server, "kmp_ingest", canonical).await;
    assert_eq!(written["isError"], false, "{written}");
    let updated = call(&server,"kmp_write_memory",json!({"about":ABOUT,"actor":"writer","idempotency_key":"render-unknown-observation",
        "search_summaries":[{"ref":preview["structuredContent"]["local_refs"]["one"],"summary_en":"The cache service stopped working."}]})).await;
    assert_eq!(updated["isError"], false, "{updated}");
    let detail = receipt(&server, &updated).await;
    let coordinate = &detail["canonical_memory"]["entries"][0]["coordinates"][0];
    assert!(coordinate.get("observed_at").is_none());
    assert!(coordinate.get("occurred_at").is_none());
    assert!(coordinate["ingested_at"].is_string());
    assert!(detail["provenance"]["observed_at"].is_string());
    assert_eq!(updated["structuredContent"]["clock_defaults"]["entries"], 0);
}
