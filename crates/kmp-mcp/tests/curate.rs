use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn call(server: &KernelMcpServer, id: u64, name: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc":"2.0", "id":id, "method":"tools/call",
        "params":{"name":name, "arguments":arguments}})
    .to_string();
    let response = server.handle_json_line(&line).await.expect("MCP response");
    let value: Value = serde_json::from_str(&response).expect("JSON");
    assert!(value.get("error").is_none(), "{value}");
    assert_ne!(value["result"]["isError"], true, "{value}");
    value["result"]["structuredContent"].clone()
}

fn seed(about: &str, texts: &[&str]) -> Value {
    let entries = texts
        .iter()
        .enumerate()
        .map(|(n, text)| {
            json!({
                "id": format!("{about}:entry:{n}"), "kind": "observation", "text": text,
                "coordinates": [{"dimension": "task", "scope_id": "test", "sequence": n + 1,
                    "occurred_at": "2026-01-01T00:00:00Z"}]
            })
        })
        .collect::<Vec<_>>();
    json!({"about": about, "idempotency_key": format!("seed:{about}"), "memory": {
        "dimensions": [{"id": "test", "kind": "task"}], "entries": entries,
        "relations": [], "evidence": []}})
}

/// Without `typesafe.json` the review still works: kernel pairs come back
/// untyped, nothing is audited, and the frozen review pages by token.
#[tokio::test]
async fn review_without_jev_lists_kernel_pairs_and_freezes_its_pages() {
    let dir = tempfile::tempdir().expect("dir");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    call(
        &server,
        1,
        "kmp_ingest",
        seed(
            "service:alpha",
            &[
                "Ticket #4711 moved the cache to cluster mode.",
                "Latency dropped after #4711 shipped.",
                "The canteen menu was posted.",
            ],
        ),
    )
    .await;
    call(
        &server,
        2,
        "kmp_ingest",
        seed(
            "service:beta",
            &[
                "The Valkey cluster was upgraded by Ana Ruiz.",
                "Ana Ruiz documented the Valkey upgrade.",
                "Quarterly planning closed.",
            ],
        ),
    )
    .await;
    let first = call(
        &server,
        3,
        "kmp_curate",
        json!({"mode": "review", "about": "service:alpha",
        "dimensions": {"scope": "abouts", "abouts": ["service:alpha", "service:beta"]},
        "page": {"entries": 1}}),
    )
    .await;
    assert!(first["jev"].is_null(), "{first}");
    assert!(first["warnings"].to_string().contains("Jev"), "{first}");
    let missing = first["missing"].as_array().expect("missing");
    assert_eq!(missing.len(), 1, "{first}");
    assert!(missing[0]["suggested_rel"].is_null());
    assert_eq!(missing[0]["proposed_by"], "kernel");
    assert!(
        first["page"]["total"].as_u64().expect("total") >= 2,
        "{first}"
    );
    let token = first["review_token"].as_str().expect("token").to_string();
    let cursor = first["page"]["next_cursor"]
        .as_str()
        .expect("more")
        .to_string();
    let second = call(
        &server,
        4,
        "kmp_curate",
        json!({"mode": "review", "about": "service:alpha",
        "review_token": token, "page": {"entries": 1, "cursor": cursor}}),
    )
    .await;
    assert_eq!(second["review_token"], first["review_token"]);
    assert_ne!(second["missing"], first["missing"], "{second}");
}

#[tokio::test]
async fn an_unknown_review_token_asks_for_a_fresh_review() {
    let dir = tempfile::tempdir().expect("dir");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let line =
        json!({"jsonrpc":"2.0", "id":1, "method":"tools/call", "params":{"name":"kmp_curate",
        "arguments":{"mode":"review","about":"service:alpha","review_token":"0".repeat(64)}}})
        .to_string();
    let response = server.handle_json_line(&line).await.expect("MCP response");
    assert!(response.contains("review expired"), "{response}");
}

async fn seeded() -> (tempfile::TempDir, KernelMcpServer) {
    let dir = tempfile::tempdir().expect("dir");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    call(
        &server,
        1,
        "kmp_ingest",
        seed(
            "service:alpha",
            &[
                "Ticket #4711 moved the cache to cluster mode.",
                "Latency dropped after #4711 shipped.",
                "The canteen menu was posted.",
            ],
        ),
    )
    .await;
    call(
        &server,
        2,
        "kmp_ingest",
        seed(
            "service:beta",
            &[
                "The Valkey cluster was upgraded by Ana Ruiz.",
                "Ana Ruiz documented the Valkey upgrade.",
                "Quarterly planning closed.",
            ],
        ),
    )
    .await;
    (dir, server)
}

async fn raw(server: &KernelMcpServer, name: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc":"2.0", "id":9, "method":"tools/call",
        "params":{"name":name, "arguments":arguments}})
    .to_string();
    serde_json::from_str(&server.handle_json_line(&line).await.expect("MCP response"))
        .expect("JSON")
}

/// The agent accepts one missing item in its own words; the writer's review
/// asks for a look, the returned action resumes the same apply, and the link
/// lands with its provenance. Repeating the call replays it.
#[tokio::test]
async fn an_applied_item_is_reviewed_resumed_committed_and_replayed() {
    let (_dir, server) = seeded().await;
    let review = call(
        &server,
        3,
        "kmp_curate",
        json!({"mode": "review", "about": "service:alpha",
        "dimensions": {"scope": "abouts", "abouts": ["service:alpha", "service:beta"]}}),
    )
    .await;
    let item = review["missing"]
        .as_array()
        .expect("missing")
        .iter()
        .find(|item| {
            item["from"]["about"] == "service:alpha" && item["to"]["about"] == "service:alpha"
        })
        .cloned()
        .expect("an item inside alpha");
    let apply = json!({"mode": "apply", "about": "service:alpha", "actor": "curator",
        "review_token": review["review_token"],
        "accepted": [{"item_id": item["item_id"], "rel": "supports",
            "why": "The latency drop is reported right after the #4711 change shipped.",
            "evidence": "Both entries name #4711; the second reports its effect."}]});
    let first = call(&server, 4, "kmp_curate", apply.clone()).await;
    assert_eq!(first["status"], "needs_review", "{first}");
    let resume = &first["next_actions"][0];
    assert_eq!(resume["tool"], "kmp_curate", "{first}");
    assert!(
        resume["arguments"]["write_review_token"].is_string(),
        "{first}"
    );
    let committed = call(&server, 5, "kmp_curate", resume["arguments"].clone()).await;
    assert_eq!(committed["status"], "committed", "{committed}");
    assert_eq!(committed["curate"]["rejected"], json!([]));
    let evidence = committed["attachment"]["created"]["evidence"][0]
        .as_str()
        .expect("evidence ref")
        .to_string();
    let inspected = call(
        &server,
        8,
        "kmp_inspect",
        json!({"about": "service:alpha", "ref": evidence,
        "include": {"details": true, "incoming": false, "outgoing": false, "raw": true}}),
    )
    .await;
    assert!(inspected.to_string().contains("curated_by"), "{inspected}");
    assert!(inspected.to_string().contains("kmp_curate"), "{inspected}");

    let related = call(
        &server,
        6,
        "kmp_relate",
        json!({"about": "service:alpha",
        "page": {"entries": 50}}),
    )
    .await;
    assert!(
        related["declared"]
            .to_string()
            .contains("#4711 change shipped"),
        "{related}"
    );

    let again = call(&server, 7, "kmp_curate", resume["arguments"].clone()).await;
    assert_eq!(again["status"], "replayed", "{again}");
}

#[tokio::test]
async fn items_owned_by_another_about_are_rejected_and_nothing_is_written() {
    let (_dir, server) = seeded().await;
    let review = call(
        &server,
        3,
        "kmp_curate",
        json!({"mode": "review", "about": "service:alpha",
        "dimensions": {"scope": "abouts", "abouts": ["service:alpha", "service:beta"]}}),
    )
    .await;
    let foreign = review["missing"]
        .as_array()
        .expect("missing")
        .iter()
        .find(|item| item["from"]["about"] == "service:beta")
        .cloned()
        .expect("an item owned by beta");
    let result = call(&server, 4, "kmp_curate", json!({"mode": "apply", "about": "service:alpha",
        "actor": "curator", "review_token": review["review_token"],
        "accepted": [{"item_id": foreign["item_id"], "why": "w", "evidence": "e", "rel": "supports"}]})).await;
    assert_eq!(result["status"], "nothing_written", "{result}");
    assert!(
        result["curate"]["rejected"][0]["reason"]
            .as_str()
            .expect("reason")
            .contains("another about")
    );
}

#[tokio::test]
async fn the_internal_prepare_mode_is_not_callable_from_outside() {
    let (_dir, server) = seeded().await;
    let response = raw(
        &server,
        "kmp_curate",
        json!({"mode": "prepare_apply", "about": "service:alpha"}),
    )
    .await;
    assert_eq!(response["result"]["isError"], true, "{response}");
}

#[tokio::test]
async fn apply_without_an_actor_or_context_is_refused() {
    let (_dir, server) = seeded().await;
    let response = raw(&server, "kmp_curate", json!({"mode": "apply", "about": "service:alpha",
        "review_token": "0".repeat(64), "accepted": [{"item_id": "m0", "why": "w", "evidence": "e"}]})).await;
    assert_eq!(response["result"]["isError"], true, "{response}");
    assert!(response.to_string().contains("actor"), "{response}");
}
