//! Frozen continuations (P2): a Wake or Ask continuation cuts its page from
//! the first page's read while the store stands still, byte for byte what a
//! fresh read returns, and reads again once anything was committed.

#[path = "support/reviewed_writer.rs"]
mod reviewed_writer;

use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};
use std::path::Path;

const ABOUT: &str = "project:frozen-pages";
const OTHER: &str = "project:frozen-elsewhere";

fn tool_call(id: u64, name: &str, arguments: &Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments}
    })
    .to_string()
}

fn many_pages(about: &str, key: &str, evidence: usize) -> Value {
    let claim = format!("{about}:claim:rollout");
    let evidence = (0..evidence)
        .map(|index| {
            json!({
                "id": format!("evidence:{about}:note:{index:03}"),
                "supports": [claim],
                "text": format!(
                    "Rollout note {index}: the gate held because contract tests for shard {index} were missing."
                ),
                "source": format!("rollout log {index:03}")
            })
        })
        .collect::<Vec<_>>();
    json!({
        "about": about,
        "idempotency_key": key,
        "memory": {
            "dimensions": [{"id": "work:rollout", "kind": "work"}],
            "entries": [
                {
                    "id": claim,
                    "kind": "claim",
                    "text": "The rollout gate stays closed until every shard has contract tests.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:rollout",
                        "occurred_at": "2026-09-01T00:00:00Z",
                        "sequence": 1
                    }]
                },
                {
                    "id": format!("{about}:claim:next"),
                    "kind": "claim",
                    "text": "Write the missing contract tests before reopening the gate.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:rollout",
                        "occurred_at": "2026-09-01T00:00:01Z",
                        "sequence": 2
                    }]
                }
            ],
            "relations": [{
                "from": claim,
                "to": format!("{about}:claim:next"),
                "rel": "triggers",
                "class": "causal",
                "why": "A closed gate makes the missing tests the next required work.",
                "evidence": "The gate review names the missing contract tests.",
                "confidence": "high"
            }],
            "evidence": evidence
        }
    })
}

fn one_more_entry(about: &str, key: &str) -> Value {
    json!({
        "about": about,
        "idempotency_key": key,
        "memory": {
            "dimensions": [{"id": "work:rollout", "kind": "work"}],
            "entries": [{
                "id": format!("{about}:claim:late"),
                "kind": "claim",
                "text": "A late decision: shard seven is exempt from the gate.",
                "coordinates": [{
                    "dimension": "work",
                    "scope_id": "work:rollout",
                    "occurred_at": "2026-09-02T00:00:00Z",
                    "sequence": 3
                }]
            }]
        }
    })
}

async fn raw(server: &KernelMcpServer, id: u64, name: &str, arguments: &Value) -> String {
    server
        .handle_json_line(&tool_call(id, name, arguments))
        .await
        .expect("tool call should produce a response")
}

async fn write(server: &KernelMcpServer, id: u64, arguments: &Value) {
    let reply: Value =
        serde_json::from_str(&raw(server, id, "kmp_ingest", arguments).await).expect("JSON");
    assert!(reply.get("error").is_none(), "{reply}");
    let result = reviewed_writer::review_authored_write(
        server,
        reply["result"]["structuredContent"].clone(),
    )
    .await;
    assert!(
        result["memory"]["read_after_write_ready"] == true || result["status"] == "committed",
        "{result}"
    );
}

/// Continuation handles are minted at random per call; everything else must
/// be the same bytes.
fn without_handles(response: &str) -> String {
    let mut out = String::with_capacity(response.len());
    let mut rest = response;
    while let Some(at) = rest.find("read_") {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 5..];
        let hex = tail
            .bytes()
            .take_while(|byte| byte.is_ascii_hexdigit())
            .count();
        if hex == 32 {
            out.push_str("read_<handle>");
            rest = &tail[32..];
        } else {
            out.push_str("read_");
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

fn next_action(response: &str) -> Option<(String, Value)> {
    let value: Value = serde_json::from_str(response).expect("JSON");
    let action = value["result"]["structuredContent"]["projection"]["next_action"].clone();
    action["tool"]
        .as_str()
        .map(|tool| (tool.to_string(), action["arguments"].clone()))
}

/// The page proposes a cursor continuation, not a restart with more bytes.
fn cursor_proposed(response: &str) -> bool {
    let value: Value = serde_json::from_str(response).expect("JSON");
    let projection = &value["result"]["structuredContent"]["projection"];
    projection["page"]["next_cursor"].is_string() && projection["core_text_shortened"] != true
}

fn is_error(response: &str) -> bool {
    let value: Value = serde_json::from_str(response).expect("JSON");
    value.get("error").is_some() || value["result"]["isError"] == true
}

/// The same call on a server that has never seen this read: the reference
/// a frozen page must equal.
async fn fresh(dir: &Path, id: u64, name: &str, arguments: &Value) -> String {
    let server = KernelMcpServer::embedded(dir).expect("fresh server opens");
    raw(&server, id, name, arguments).await
}

async fn seeded() -> (tempfile::TempDir, KernelMcpServer) {
    let dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded server opens");
    write(&server, 1, &many_pages(ABOUT, "ingest:frozen-pages", 48)).await;
    write(&server, 2, &many_pages(OTHER, "ingest:frozen-elsewhere", 4)).await;
    (dir, server)
}

#[tokio::test]
async fn frozen_wake_and_ask_pages_equal_fresh_reads_byte_for_byte() {
    let (dir, server) = seeded().await;
    for (tool, first) in [
        (
            "kmp_wake",
            json!({"about": ABOUT, "budget": {"max_bytes": 2048}}),
        ),
        (
            "kmp_ask",
            json!({"about": ABOUT, "question": "Why does the rollout gate stay closed?", "budget": {"max_bytes": 2048}}),
        ),
    ] {
        let mut id = 100;
        let mut step = Some((tool.to_string(), first));
        let mut pages = 0;
        while let Some((name, arguments)) = step {
            id += 1;
            let served = raw(&server, id, &name, &arguments).await;
            let reference = fresh(dir.path(), id, &name, &arguments).await;
            assert!(!is_error(&served), "{name} page {pages}: {served}");
            assert_eq!(
                without_handles(&served),
                without_handles(&reference),
                "{name} page {pages} differs from a fresh read"
            );
            pages += 1;
            assert!(pages < 64, "{name} must finish");
            step = next_action(&served);
        }
        assert!(pages > 2, "{tool} fixture must page: {pages}");
    }
}

#[tokio::test]
async fn a_write_between_pages_makes_the_continuation_read_again() {
    let (dir, server) = seeded().await;
    let first = json!({"about": ABOUT, "budget": {"max_bytes": 4096}});
    let page = raw(&server, 10, "kmp_wake", &first).await;
    assert!(
        cursor_proposed(&page),
        "the fixture must page by cursor: {page}"
    );
    let (name, continuation) = next_action(&page).expect("first page continues");
    // Frozen: the second page is cut from the first page's read.
    let second = raw(&server, 11, &name, &continuation).await;
    assert_eq!(
        without_handles(&second),
        without_handles(&fresh(dir.path(), 11, &name, &continuation).await)
    );
    assert!(cursor_proposed(&second), "{second}");
    assert!(!is_error(&second), "{second}");
    let (name, third) = next_action(&second).expect("second page continues");

    // A commit to another about moves the store's revision: the page is read
    // again, and equals a fresh read because this about did not change.
    write(&server, 12, &one_more_entry(OTHER, "ingest:elsewhere-late")).await;
    let after_other = raw(&server, 13, &name, &third).await;
    assert!(!is_error(&after_other), "{after_other}");
    assert_eq!(
        without_handles(&after_other),
        without_handles(&fresh(dir.path(), 13, &name, &third).await)
    );
    assert!(cursor_proposed(&after_other), "{after_other}");
    let (name, fourth) = next_action(&after_other).expect("third page continues");

    // A commit to this about changes its selection. A frozen page would
    // still be served; a read of the current store refuses the stale cursor
    // exactly as the unfrozen server does.
    write(&server, 14, &one_more_entry(ABOUT, "ingest:frozen-late")).await;
    let after_write = raw(&server, 15, &name, &fourth).await;
    let reference = fresh(dir.path(), 15, &name, &fourth).await;
    assert!(is_error(&reference), "{reference}");
    assert!(
        is_error(&after_write),
        "a continuation after a write must be read again: {after_write}"
    );
    assert_eq!(without_handles(&after_write), without_handles(&reference));
}

#[tokio::test]
async fn a_repeated_first_page_and_a_repeat_core_page_are_unchanged() {
    let (dir, server) = seeded().await;
    let first = json!({"about": ABOUT, "budget": {"max_bytes": 4096}});
    let page = raw(&server, 20, "kmp_wake", &first).await;
    assert!(cursor_proposed(&page), "{page}");
    // A call without a cursor always reads, even with the read frozen.
    let again = raw(&server, 20, "kmp_wake", &first).await;
    assert_eq!(without_handles(&page), without_handles(&again));
    let (name, continuation) = next_action(&page).expect("first page continues");
    let mut repeat = continuation.clone();
    repeat["page"] = json!({"repeat_core": true});
    let served = raw(&server, 22, &name, &repeat).await;
    assert!(!is_error(&served), "{served}");
    assert_eq!(
        without_handles(&served),
        without_handles(&fresh(dir.path(), 22, &name, &repeat).await)
    );
}
