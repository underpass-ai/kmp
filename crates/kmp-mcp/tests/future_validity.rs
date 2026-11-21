//! B4 of the 1.0 review: a memory valid from next year used to move the
//! about's "now" forward, so the memory in force this year was served as
//! expired with `high` confidence. The present is the latest observation; a
//! memory whose `valid_from` is still ahead is recorded but not yet in force.

use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn call(server: &KernelMcpServer, id: u64, name: &str, arguments: Value) -> Value {
    let reply = server
        .handle_json_line(
            &json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                    "params": {"name": name, "arguments": arguments}})
            .to_string(),
        )
        .await
        .expect("answered");
    let reply: Value = serde_json::from_str(&reply).expect("JSON");
    assert_ne!(reply["result"]["isError"], true, "{name}: {reply}");
    reply["result"]["structuredContent"].clone()
}

fn rota(who: &str, observed: &str, from: &str, until: Option<&str>) -> Value {
    let mut memory = json!({"id": "m", "kind": "observation",
        "summary": format!("The on-call engineer is {who}."),
        "evidence": format!("Rota sheet, row for {who}."),
        "observed_at": observed, "valid_from": from});
    if let Some(until) = until {
        memory["valid_until"] = json!(until);
    }
    json!({"about": "team:rota", "actor": "tester", "labels": {"topic": ["rota"]},
           "idempotency_key": format!("rota-{who}"), "memories": [memory]})
}

#[tokio::test]
async fn the_memory_in_force_is_served_and_the_future_one_waits() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    call(
        &server,
        1,
        "kmp_write_memory",
        rota(
            "Marta",
            "2026-11-01T10:00:00Z",
            "2026-01-01T00:00:00Z",
            Some("2026-12-31T23:59:59Z"),
        ),
    )
    .await;
    call(
        &server,
        2,
        "kmp_write_memory",
        rota(
            "Diego",
            "2026-11-01T10:05:00Z",
            "2027-01-01T00:00:00Z",
            None,
        ),
    )
    .await;

    let asked = call(
        &server,
        3,
        "kmp_ask",
        json!({"about": "team:rota", "question": "Who is the on-call engineer?"}),
    )
    .await;
    let cited = asked["because"].to_string();
    assert!(cited.contains("marta"), "{asked}");
    assert!(!cited.contains("diego"), "{asked}");
    assert_eq!(asked["proof"]["expired"], json!([]), "{asked}");

    let woken = call(&server, 4, "kmp_wake", json!({"about": "team:rota"})).await;
    // Wake lists what the lifecycle set aside after the live memories, as it
    // does an expired one: the memory in force leads.
    let current = woken["wake"]["current_state"].to_string();
    let marta = current
        .find("Marta")
        .expect("the memory in force is listed");
    assert!(
        current.find("Diego").is_none_or(|diego| marta < diego),
        "the memory not yet in force follows the live one: {woken}"
    );
    // And it says what it is: wherever it is shown, a memory not yet in
    // force is labelled so and never reads as current state.
    for line in woken["wake"]["current_state"].as_array().expect("lines") {
        let line = line.as_str().expect("line");
        if line.contains("Diego") {
            assert!(line.contains("not yet in force"), "{line}");
        } else if line.contains("Marta") {
            assert!(!line.contains("not yet in force"), "{line}");
        }
    }

    let related = call(&server, 5, "kmp_relate", json!({"about": "team:rota"})).await;
    let states = related["facts"]
        .as_array()
        .expect("facts")
        .iter()
        .map(|fact| {
            (
                fact["text"].as_str().unwrap_or_default().to_string(),
                fact["state"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect::<Vec<_>>();
    for (text, state) in &states {
        let expected = if text.contains("Diego") {
            "not_yet_in_force"
        } else {
            "current"
        };
        assert_eq!(state, expected, "{text}: {related}");
    }
    assert_eq!(states.len(), 2, "{related}");
}
