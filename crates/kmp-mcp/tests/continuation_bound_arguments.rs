//! A continuation stands for the call it was returned with. Beside it an
//! agent may resend any argument of that call with the same value (it often
//! repeats the `about`, `move` or `page.entries` of its first call); the
//! answer is then the continuation's own, byte for byte. A value that call
//! does not have would be a different read: it is refused under the rule
//! `continuation-alone`, naming the argument.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:bound-arguments";

async fn raw(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}});
    let line = server
        .handle_json_line(&request.to_string())
        .await
        .expect("reply");
    serde_json::from_str::<Value>(&line).expect("JSON")["result"].clone()
}

async fn seeded() -> (tempfile::TempDir, KernelMcpServer) {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let memories = (0..8)
        .map(|index| {
            json!({"id":format!("m{index}"),"kind":"observation",
                "summary":format!("Observation {index} about the export pipeline."),
                "evidence":format!("Log {index} records the export pipeline step {index}. ").repeat(6),
                "occurred_at":format!("2026-09-01T{:02}:00:00Z", 1 + index)})
        })
        .collect::<Vec<_>>();
    let written = raw(
        &server,
        "kmp_write_memory",
        json!({"about":ABOUT,"actor":"bound","observed_at":"2026-09-01T23:00:00Z",
            "idempotency_key":"bound:seed","labels":{"component":["export"]},"memories":memories}),
    )
    .await;
    assert_eq!(
        written["structuredContent"]["status"], "committed",
        "{written}"
    );
    (dir, server)
}

fn handle(action: &Value) -> String {
    action["arguments"]["continuation"]
        .as_str()
        .unwrap_or_else(|| panic!("a continuation handle: {action}"))
        .to_string()
}

fn with(handle: &str, extra: Value) -> Value {
    let mut arguments = extra;
    arguments["continuation"] = json!(handle);
    arguments
}

/// The refusal is the rule's and names the argument that differs.
fn assert_refused_naming(reply: &Value, path: &str) {
    assert_eq!(reply["isError"], true, "{reply}");
    let content = &reply["structuredContent"];
    assert_eq!(content["error"]["code"], "invalid_argument", "{reply}");
    let message = content["error"]["message"].as_str().expect("message");
    assert!(
        message.starts_with("Rule `continuation-alone`: "),
        "{message}"
    );
    assert!(
        message.contains(&format!("`{path}`")),
        "names `{path}`: {message}"
    );
    assert!(message.contains("page.cursor"), "{message}");
    assert!(
        content["feedback"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|item| {
                item["rule"] == "continuation-alone"
                    && item["field"] == path
                    && item["code"] == "CONTINUATION_ARGUMENT_DIFFERS"
            }),
        "{reply}"
    );
}

#[tokio::test]
async fn a_history_continuation_accepts_the_move_it_was_made_with() {
    let (_dir, server) = seeded().await;
    let first = raw(
        &server,
        "kmp_time",
        json!({"about":ABOUT,"move":"rewind","axis":"occurred","from":{"time":"2026-09-02T00:00:00Z"},
            "include":{"evidence":true},"budget":{"max_bytes":1500}}),
    )
    .await;
    assert_eq!(
        first["structuredContent"]["page"]["has_more"], true,
        "{first}"
    );
    let id = handle(&first["structuredContent"]["next_actions"][0]);

    let alone = raw(&server, "kmp_time", with(&id, json!({}))).await;
    assert_eq!(alone["isError"], false, "{alone}");
    let same = raw(
        &server,
        "kmp_time",
        with(
            &id,
            json!({"about":ABOUT,"move":"rewind","axis":"occurred"}),
        ),
    )
    .await;
    assert_eq!(
        same, alone,
        "an equal argument changes nothing: the answer is the continuation's own"
    );

    let other = raw(
        &server,
        "kmp_time",
        with(&id, json!({"about":ABOUT,"move":"forward"})),
    )
    .await;
    assert_refused_naming(&other, "move");
}

#[tokio::test]
async fn a_recall_continuation_accepts_its_page_size_and_refuses_another() {
    let (_dir, server) = seeded().await;
    let first = raw(
        &server,
        "kmp_wake",
        json!({"about":ABOUT,"budget":{"max_bytes":4000,"detail":"full"},"page":{"entries":2}}),
    )
    .await;
    let action = &first["structuredContent"]["projection"]["next_action"];
    let id = handle(action);

    let alone = raw(&server, "kmp_wake", with(&id, json!({}))).await;
    assert_eq!(alone["isError"], false, "{alone}");
    let same = raw(
        &server,
        "kmp_wake",
        with(&id, json!({"about":ABOUT,"page":{"entries":2}})),
    )
    .await;
    assert_eq!(same, alone, "page.entries: 2 is the page size it binds");

    let other = raw(
        &server,
        "kmp_wake",
        with(&id, json!({"page":{"entries":3}})),
    )
    .await;
    assert_refused_naming(&other, "page.entries");

    // An argument the original call did not send is a different read too.
    let added = raw(
        &server,
        "kmp_wake",
        with(&id, json!({"as_of":{"time":"2026-09-01T05:00:00Z"}})),
    )
    .await;
    assert_refused_naming(&added, "as_of");
    assert!(
        added["structuredContent"]["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("does not carry it")),
        "{added}"
    );

    // page.repeat_core keeps its meaning beside an equal argument.
    let rehydrated = raw(
        &server,
        "kmp_wake",
        with(
            &id,
            json!({"about":ABOUT,"page":{"entries":2,"repeat_core":true}}),
        ),
    )
    .await;
    assert_eq!(rehydrated["isError"], false, "{rehydrated}");
    assert!(
        rehydrated["structuredContent"]["projection"]
            .get("core_reused")
            .is_none(),
        "{rehydrated}"
    );
}
