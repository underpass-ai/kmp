//! QA, 13 nov 2026: `kmp_write_memory` with `options.dry_run: "true"`
//! answered "Ingested 1 entry" and committed a memory the caller meant only
//! to preview. A write switch of the wrong type is now refused, on every
//! write shape and every write tool, and nothing reaches the store.

use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": tool, "arguments": arguments}});
    let reply = server
        .handle_json_line(&request.to_string())
        .await
        .expect("reply");
    serde_json::from_str::<Value>(&reply).expect("JSON")["result"].clone()
}

fn text(result: &Value) -> String {
    result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|content| content["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The QA replay's packet.
fn packet(options: Option<Value>) -> Value {
    let mut packet = json!({
        "about": "project:qa-sandbox", "actor": "qa", "labels": {"topic": ["qa"]},
        "memories": [{"id": "m", "kind": "observation", "summary": "QA test",
            "evidence": "sandbox probe"}]
    });
    if let Some(options) = options {
        packet["options"] = options;
    }
    packet
}

async fn event_count(store: &EmbeddedKernelStore) -> u64 {
    let bundle = store.export_bundle().await.expect("export");
    kmp_embedded::verify_bundle(&bundle)
        .expect("header")
        .event_count
}

#[tokio::test]
async fn a_mistyped_dry_run_is_refused_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let before = event_count(&store).await;

    let refusals = [
        ("options.dry_run", json!({"dry_run": "true"})),
        ("options.dry_run", json!({"dry_run": "false"})),
        ("options.dry_run", json!({"dry_run": 1})),
        ("options.dry_run", json!({"dry_run": null})),
        (
            "options.strict",
            json!({"strict": "false", "dry_run": true}),
        ),
        ("options", json!("{\"dry_run\":true}")),
    ];
    for (field, options) in refusals {
        let refused = call(&server, "kmp_write_memory", packet(Some(options.clone()))).await;
        assert_eq!(refused["isError"], true, "{options}: {refused}");
        assert!(
            text(&refused).contains(field),
            "{options}: the refusal names `{field}`: {refused}"
        );
        assert!(!text(&refused).contains("Ingested"), "{options}: {refused}");
    }
    assert_eq!(event_count(&store).await, before, "nothing was committed");

    // Controls: the boolean preview writes nothing; the plain call commits.
    let preview = call(
        &server,
        "kmp_write_memory",
        packet(Some(json!({"dry_run": true}))),
    )
    .await;
    assert_eq!(preview["isError"], false, "{preview}");
    assert!(text(&preview).starts_with("Prepared"), "{preview}");
    assert_eq!(
        event_count(&store).await,
        before,
        "a preview writes nothing"
    );

    let committed = call(
        &server,
        "kmp_write_memory",
        packet(Some(json!({"dry_run": false}))),
    )
    .await;
    assert_eq!(committed["isError"], false, "{committed}");
    assert!(text(&committed).starts_with("Ingested"), "{committed}");
    assert!(event_count(&store).await > before, "the commit is stored");
}

#[tokio::test]
async fn every_write_shape_and_tool_refuses_a_mistyped_switch() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let before = event_count(&store).await;
    let observed = "2026-11-13T03:30:38Z";

    let refusals = [
        (
            "search_summaries",
            "kmp_write_memory",
            json!({
                "about": "project:qa-sandbox", "actor": "qa", "observed_at": observed,
                "search_summaries": [{"ref": "project:qa-sandbox:observation:a",
                    "summary_en": "The probe ran in the sandbox."}],
                "options": {"dry_run": "true"}
            }),
            "options.dry_run",
        ),
        (
            "search_expansions",
            "kmp_write_memory",
            json!({
                "about": "project:qa-sandbox", "actor": "qa", "observed_at": observed,
                "search_summaries": [{"ref": "project:qa-sandbox:observation:a",
                    "search_expansions": {"entities": ["sandbox"]}}],
                "options": {"dry_run": "true"}
            }),
            "options.dry_run",
        ),
        (
            "relations",
            "kmp_write_memory",
            json!({
                "about": "project:qa-sandbox", "actor": "qa", "observed_at": observed,
                "relations": [{"from": "project:qa-sandbox:observation:a",
                    "to": "project:qa-sandbox:decision:b", "rel": "supports",
                    "class": "evidential", "confidence": "high",
                    "why": "The probe is why.", "evidence": "Report R1."}],
                "options": {"dry_run": "true"}
            }),
            "options.dry_run",
        ),
        (
            "relabel",
            "kmp_relabel",
            json!({
                "about": "project:qa-sandbox", "ref": "project:qa-sandbox:observation:a",
                "actor": "qa", "observed_at": observed, "why": "It is a QA memory.",
                "add": {"topic": ["qa"]}, "options": {"dry_run": "true"}
            }),
            "dry_run",
        ),
        (
            "ingest",
            "kmp_ingest",
            json!({
                "about": "project:qa-sandbox", "dry_run": "true",
                "memory": {"dimensions": [], "entries": [], "relations": [], "evidence": []}
            }),
            "dry_run",
        ),
        (
            "curate",
            "kmp_curate",
            json!({
                "mode": "apply", "about": "project:qa-sandbox",
                "review_token": "0".repeat(64),
                "accepted": [{"item_id": "m1", "why": "The probe is why.",
                    "evidence": "Report R1.", "reverse": "true"}]
            }),
            "reverse",
        ),
    ];
    for (shape, tool, arguments, field) in refusals {
        let refused = call(&server, tool, arguments).await;
        assert_eq!(refused["isError"], true, "{shape}: {refused}");
        assert!(
            text(&refused).contains(field),
            "{shape} names `{field}`: {refused}"
        );
    }
    assert_eq!(event_count(&store).await, before, "nothing was committed");
}
