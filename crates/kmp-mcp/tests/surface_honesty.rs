//! What the agent is told matches what the code does (E4, E7, E11, E12,
//! E21, E23, E26 of the 1.0 exposure review), on the embedded kernel.

use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn rpc(server: &KernelMcpServer, line: Value) -> Value {
    let reply = server
        .handle_json_line(&line.to_string())
        .await
        .expect("reply");
    serde_json::from_str::<Value>(&reply).expect("JSON")
}

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    rpc(
        server,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
               "params":{"name":tool,"arguments":arguments}}),
    )
    .await["result"]
        .clone()
}

fn refusal(result: &Value) -> (String, String) {
    assert_eq!(result["isError"], true, "{result}");
    let error = &result["structuredContent"]["error"];
    (
        error["code"].as_str().unwrap_or_default().to_string(),
        error["message"].as_str().unwrap_or_default().to_string(),
    )
}

fn scratch() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&root).expect("scratch");
    tempfile::tempdir_in(root).expect("isolated store")
}

fn ingest(about: &str, key: &str) -> Value {
    json!({"about":about,"idempotency_key":key,
        "memory":{"dimensions":[{"id":"t1","kind":"task"}],
            "entries":[{"id":format!("{about}:entry:one"),"kind":"observation",
                "text":"Deploy failed on node 7.",
                "coordinates":[{"dimension":"task","scope_id":"t1"}]}]},
        "provenance":{"source_kind":"agent","source_agent":"tester",
            "observed_at":"2026-11-20T10:00:00Z"}})
}

fn packet(key: &str, link: Value) -> Value {
    json!({"about":"proj:honest","actor":"tester","idempotency_key":key,
    "labels":{"task":["t1"]},"observed_at":"2026-11-20T11:00:00Z",
    "memories":[
        {"id":"one","kind":"observation","summary":"The cache failed.",
         "evidence":"R1 reports the cache failure."},
        {"id":"two","kind":"observation","summary":"The retry succeeded.",
         "evidence":"R2 reports a successful retry after R1.","connect_to":[link]}
    ]})
}

async fn receipt(server: &KernelMcpServer, written: &Value) -> Value {
    let action = &written["structuredContent"]["receipt"]["action"];
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
async fn an_unknown_about_is_named_as_an_about_with_one_code_on_every_read() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    assert_eq!(
        call(&server, "kmp_ingest", ingest("proj:known", "k0")).await["isError"],
        false
    );
    let reads = [
        (
            "kmp_ask",
            json!({"about":"proj:nope","question":"what failed?"}),
        ),
        ("kmp_relate", json!({"about":"proj:nope"})),
        (
            "kmp_time",
            json!({"about":"proj:nope","move":"rewind","from":{"time":"2026-12-01T00:00:00Z"}}),
        ),
        (
            "kmp_trace",
            json!({"about":"proj:nope","from":"x","to":"y"}),
        ),
        ("kmp_inspect", json!({"about":"proj:nope","ref":"x"})),
        ("kmp_curate", json!({"about":"proj:nope","mode":"review"})),
        ("kmp_view_open", json!({"about":"proj:nope"})),
    ];
    for (tool, arguments) in reads {
        let (code, message) = refusal(&call(&server, tool, arguments).await);
        assert_eq!(code, "not_found", "{tool}: {message}");
        assert!(
            message.contains("`proj:nope` is not an about in this store"),
            "{tool}: {message}"
        );
        assert!(!message.contains("node"), "{tool}: {message}");
    }
    // A known about with a missing ref keeps the kernel's own refusal.
    let (_, message) = refusal(
        &call(
            &server,
            "kmp_inspect",
            json!({"about":"proj:known","ref":"proj:known:entry:absent"}),
        )
        .await,
    );
    assert!(!message.contains("is not an about"), "{message}");
}

#[tokio::test]
async fn a_writer_clock_is_refused_at_the_path_the_caller_wrote() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut bad = packet(
        "clock",
        json!({"ref":"one","rel":"follows","class":"procedural",
               "why":"The retry follows the failure.","evidence":"R2 follows R1."}),
    );
    bad["memories"][0]["valid_until"] = json!("tomorrow");
    let result = call(&server, "kmp_write_memory", bad).await;
    let (code, message) = refusal(&result);
    assert_eq!(code, "invalid_argument", "{result}");
    assert!(message.contains("`valid_until`"), "{message}");
    assert!(!message.contains("memory.entries"), "{message}");
    let fields = result["structuredContent"]["feedback"].to_string();
    assert!(fields.contains("memories[0].valid_until"), "{result}");
}

#[tokio::test]
async fn an_omitted_relation_confidence_is_recorded_as_unknown_never_high() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let written = call(
        &server,
        "kmp_write_memory",
        packet(
            "confidence",
            json!({"ref":"one","rel":"follows","class":"procedural",
                   "why":"The retry follows the failure.","evidence":"R2 follows R1."}),
        ),
    )
    .await;
    assert_eq!(written["isError"], false, "{written}");
    let first = receipt(&server, &written).await;
    let link = &first["canonical_memory"]["relations"][0];
    assert_eq!(link["rel"], "follows", "{first}");
    assert_eq!(link["confidence"], "unknown", "{first}");

    // The contract says so too: tools/list leaves defaults to the guide
    // card, which serves them in the argument's line.
    let card = call(&server, "kmp_guide", json!({"topic":"relations"})).await;
    let text = card["structuredContent"].to_string();
    assert!(text.contains("unknown"), "{card}");

    // The relation-only shape records the same default.
    let linked = call(
        &server,
        "kmp_write_memory",
        json!({"about":"proj:honest","actor":"tester","idempotency_key":"relation-only",
               "observed_at":"2026-11-20T12:00:00Z",
               "relations":[{"from": first["canonical_memory"]["entries"][0]["id"],
                              "to": first["canonical_memory"]["entries"][1]["id"],
                              "rel":"follows","class":"procedural",
                              "why":"The retry follows the failure.","evidence":"R2 follows R1."}]}),
    )
    .await;
    assert_eq!(linked["isError"], false, "{linked}");
    let linked_receipt = receipt(&server, &linked).await;
    let relation = linked_receipt["canonical_memory"]["relations"]
        .as_array()
        .expect("relations")
        .iter()
        .find(|relation| relation["rel"] == "follows")
        .cloned()
        .expect("the declared link");
    assert_eq!(relation["confidence"], "unknown", "{linked_receipt}");
}

#[tokio::test]
async fn a_reused_key_says_it_may_belong_to_another_about() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    assert_eq!(
        call(&server, "kmp_ingest", ingest("proj:p", "shared-key")).await["isError"],
        false
    );
    let (code, message) =
        refusal(&call(&server, "kmp_ingest", ingest("proj:q", "shared-key")).await);
    assert_eq!(code, "conflict", "{message}");
    assert!(
        message.contains("already accepted with different content"),
        "{message}"
    );
    assert!(message.contains("possibly in another about"), "{message}");
    assert!(message.contains("global to the store"), "{message}");
}

#[tokio::test]
async fn declared_and_ignored_budget_fields_are_refused() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    call(&server, "kmp_ingest", ingest("proj:b", "b0")).await;
    for (tool, arguments) in [
        (
            "kmp_wake",
            json!({"about":"proj:b","budget":{"tokens":100}}),
        ),
        (
            "kmp_relate",
            json!({"about":"proj:b","budget":{"max_entries":3}}),
        ),
        (
            "kmp_relate",
            json!({"about":"proj:b","budget":{"detail":"full"}}),
        ),
        (
            "kmp_trace",
            json!({"about":"proj:b","from":"a","to":"b","budget":{"depth":2}}),
        ),
        (
            "kmp_trace",
            json!({"about":"proj:b","from":"a","to":"b","role":"tracer"}),
        ),
        (
            "kmp_curate",
            json!({"about":"proj:b","mode":"review","budget":{"max_bytes":4000}}),
        ),
        (
            "kmp_time",
            json!({"about":"proj:b","move":"rewind","from":{"time":"2026-12-01T00:00:00Z"},
                   "limit":{"tokens":10}}),
        ),
    ] {
        let (code, message) = refusal(&call(&server, tool, arguments.clone()).await);
        assert_eq!(code, "invalid_argument", "{tool} {arguments}: {message}");
    }
}

#[tokio::test]
async fn a_refused_intent_changes_nothing_and_keeps_its_key() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    call(&server, "kmp_ingest", ingest("proj:v", "v0")).await;
    assert_eq!(
        call(
            &server,
            "kmp_view_open",
            json!({"about":"proj:v","view_id":"e4"})
        )
        .await["isError"],
        false
    );
    let refused = call(
        &server,
        "kmp_view_apply_intent",
        json!({"view_id":"e4","idempotency_key":"z1","search":"CHANGED","explanation":""}),
    )
    .await;
    assert_eq!(refusal(&refused).0, "invalid_argument");
    let state = call(&server, "kmp_view_get_state", json!({"view_id":"e4"})).await;
    assert_eq!(
        state["structuredContent"]["state"]["search"],
        Value::Null,
        "{state}"
    );
    let retried = call(
        &server,
        "kmp_view_apply_intent",
        json!({"view_id":"e4","idempotency_key":"z1","search":"CHANGED"}),
    )
    .await;
    assert_eq!(retried["structuredContent"]["applied"], true, "{retried}");
}

#[tokio::test]
async fn a_stale_open_on_the_same_about_says_its_guard_was_not_checked() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    call(&server, "kmp_ingest", ingest("proj:o", "o0")).await;
    call(
        &server,
        "kmp_view_open",
        json!({"about":"proj:o","view_id":"e26"}),
    )
    .await;
    call(
        &server,
        "kmp_view_apply_intent",
        json!({"view_id":"e26","idempotency_key":"a1","search":"one"}),
    )
    .await;
    let reopened = call(
        &server,
        "kmp_view_open",
        json!({"about":"proj:o","view_id":"e26","expected_revision":1}),
    )
    .await;
    assert_eq!(reopened["isError"], false, "{reopened}");
    let unhonored = reopened["structuredContent"]["unhonored"].to_string();
    assert!(
        unhonored.contains("expected_revision 1 was not checked"),
        "{reopened}"
    );
    let current = call(
        &server,
        "kmp_view_open",
        json!({"about":"proj:o","view_id":"e26","expected_revision":2}),
    )
    .await;
    assert!(
        !current["structuredContent"]["unhonored"]
            .to_string()
            .contains("was not checked"),
        "{current}"
    );
}

#[tokio::test]
async fn ping_answers_empty() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let pong = rpc(&server, json!({"jsonrpc":"2.0","id":9,"method":"ping"})).await;
    assert_eq!(pong["result"], json!({}), "{pong}");
}
