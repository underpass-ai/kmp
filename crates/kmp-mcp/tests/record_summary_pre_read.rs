//! The read that `record_summary` makes before it attaches: what it asks
//! `kmp_inspect` for, and what it does when the raw record is not on the
//! first page (#497).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use kmp_mcp::{KernelMcpServer, KernelMcpToolBackend, KernelMcpToolFuture};
use serde_json::{Value, json};

const REFERENCE: &str = "project:kmp:decision:valkey";
const TEXT: &str = "Se adoptó Valkey 7.2 para el almacén compartido (ADR-018).";
const SUMMARY: &str = "Valkey 7.2 was adopted for the shared store (ADR-018).";

/// A backend that answers each call with the next scripted response and
/// keeps every call it was asked.
struct ScriptedBackend {
    calls: Arc<Mutex<Vec<(String, Value)>>>,
    responses: Mutex<VecDeque<Value>>,
}

impl KernelMcpToolBackend for ScriptedBackend {
    fn backend_name(&self) -> &'static str {
        "scripted"
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        self.calls
            .lock()
            .expect("calls are readable")
            .push((name.to_string(), arguments.clone()));
        let response = self
            .responses
            .lock()
            .expect("responses are readable")
            .pop_front()
            .unwrap_or_else(|| panic!("no scripted response left for {name}"));
        Box::pin(async move { Ok(response) })
    }
}

fn raw_record() -> Value {
    json!({
        "ref": REFERENCE,
        "kind": "decision",
        "text": TEXT,
        "coordinates": [{
            "dimension": "work",
            "scope_id": "about:project:kmp:dimension:work:main",
            "occurred_at": "2026-05-06T10:00:00Z",
            "ingested_at": "2026-05-06T10:00:01Z",
            "sequence": 3
        }],
        "detail": "",
        "content_hash": "sha256:abc",
        "revision": 1
    })
}

/// An inspect page as the projection renders it: the stable object always,
/// the raw record only when `raw` says so, and the page block that names
/// the size of the complete inspection.
fn inspect_page(raw: Vec<Value>, has_more: bool, required_bytes: u64) -> Value {
    let omitted_raw = usize::from(raw.is_empty());
    json!({
        "content": [],
        "structuredContent": {
            "summary": "one decision",
            "object": {
                "ref": REFERENCE,
                "kind": "decision",
                "text": TEXT,
                "metadata": {"writer_actor": "agent:a"}
            },
            "links": {"incoming": [], "outgoing": []},
            "evidence": [],
            "raw": raw,
            "page": {
                "offset": 0,
                "returned": 0,
                "total": 1,
                "has_more": has_more,
                "next_cursor": has_more.then_some("kmpi1:0:hash"),
                "omitted": {"details": 0, "evidence": 0, "outgoing": 0, "incoming": 0, "raw": omitted_raw},
                "sections": {"raw": {"returned_on_page": 1 - omitted_raw, "remaining": omitted_raw, "total": 1}},
                "required_bytes": required_bytes,
                "guidance": has_more.then_some("Inspect is partial.")
            },
            "quality": null,
            "warnings": []
        },
        "isError": false
    })
}

fn ingest_accepted() -> Value {
    json!({
        "content": [],
        "structuredContent": {
            "summary": "Ingested via script",
            "memory": {
                "about": "project:kmp",
                "memory_id": "memory:project:kmp:1",
                "accepted": {"entries": 1, "relations": 0, "evidence": 0},
                "read_after_write_ready": true
            },
            "warnings": []
        },
        "isError": false
    })
}

fn record_summary_request() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": {"name": "kmp_write_memory", "arguments": {
            "about": "project:kmp",
            "intent": "record_summary",
            "actor": "Codex",
            "observed_at": "2026-09-03T22:14:35Z",
            "scope": {"process": "kmp-summary-backfill-20260904"},
            "current": {"ref": REFERENCE, "summary_en": SUMMARY},
            "idempotency_key": "kmp-summary-backfill-20260904:01",
            "options": {"strict": true}
        }}
    })
}

async fn write_through(responses: Vec<Value>) -> (Value, Vec<(String, Value)>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server = KernelMcpServer::with_backend(ScriptedBackend {
        calls: Arc::clone(&calls),
        responses: Mutex::new(responses.into()),
    });
    let response = server
        .handle_json_line(&record_summary_request().to_string())
        .await
        .expect("a write answers");
    let response: Value = serde_json::from_str(&response).expect("the answer is JSON");
    let calls = calls.lock().expect("calls are readable").clone();
    (response, calls)
}

fn narrowed_include() -> Value {
    json!({"details": true, "raw": true, "incoming": false, "outgoing": false})
}

/// The case in the report: a memory with links enough that inspect, asked
/// with links included, put its raw record on a page the write never read.
/// The pre-read leaves links out; and when the record is still not on the
/// first page, it asks once more at the size the page reported and attaches
/// the stored text, kind and coordinates unchanged.
#[tokio::test]
async fn a_raw_record_pushed_off_the_first_page_is_read_at_the_size_the_page_reported() {
    let (response, calls) = write_through(vec![
        inspect_page(vec![], true, 11_403),
        inspect_page(vec![raw_record()], false, 11_403),
        ingest_accepted(),
    ])
    .await;

    assert_eq!(response["result"]["isError"], false, "{response}");
    assert_eq!(response["result"]["structuredContent"]["accepted"], true);
    assert_eq!(calls.len(), 3, "two reads and one write: {calls:?}");

    let (first, first_arguments) = &calls[0];
    assert_eq!(first, "kmp_inspect");
    assert_eq!(first_arguments["about"], "project:kmp");
    assert_eq!(first_arguments["ref"], REFERENCE);
    assert_eq!(
        first_arguments["include"],
        narrowed_include(),
        "the pre-read asks for the object and the raw record and leaves the links out"
    );
    assert!(
        first_arguments.get("budget").is_none(),
        "the first read is made under the default ceiling"
    );

    let (second, second_arguments) = &calls[1];
    assert_eq!(second, "kmp_inspect");
    assert_eq!(second_arguments["include"], narrowed_include());
    assert_eq!(
        second_arguments["budget"]["max_bytes"], 11_403,
        "the second read is made at exactly the size the first page reported"
    );

    let (third, ingest) = &calls[2];
    assert_eq!(third, "kmp_ingest");
    let entry = &ingest["memory"]["entries"][0];
    assert_eq!(entry["id"], REFERENCE);
    assert_eq!(entry["kind"], "decision");
    assert_eq!(entry["text"], TEXT);
    assert_eq!(entry["coordinates"][0]["sequence"], 3);
    assert_eq!(entry["metadata"]["summary_en"], SUMMARY);
    assert_eq!(entry["metadata"]["summary_en_by"], "Codex");
    assert_eq!(entry["metadata"]["writer_actor"], "agent:a");
}

/// A memory whose raw record fits on the first page is read once.
#[tokio::test]
async fn a_raw_record_on_the_first_page_is_read_once() {
    let (response, calls) = write_through(vec![
        inspect_page(vec![raw_record()], false, 5_368),
        ingest_accepted(),
    ])
    .await;

    assert_eq!(response["result"]["isError"], false, "{response}");
    assert_eq!(calls.len(), 2, "one read and one write: {calls:?}");
    assert_eq!(calls[0].0, "kmp_inspect");
    assert_eq!(calls[0].1["include"], narrowed_include());
    assert_eq!(calls[1].0, "kmp_ingest");
    assert_eq!(calls[1].1["memory"]["entries"][0]["text"], TEXT);
}

/// When the raw record does not come back at the reported size either, the
/// write refuses as before, naming the record that is missing, and writes
/// nothing.
#[tokio::test]
async fn a_raw_record_that_never_comes_back_still_refuses_the_write() {
    let (response, calls) = write_through(vec![
        inspect_page(vec![], true, 11_403),
        inspect_page(vec![], true, 11_403),
    ])
    .await;

    assert_eq!(response["result"]["isError"], true, "{response}");
    let message = response["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(
        message.contains("its raw record did not come back"),
        "{response}"
    );
    assert_eq!(calls.len(), 2, "two reads and no write: {calls:?}");
}
