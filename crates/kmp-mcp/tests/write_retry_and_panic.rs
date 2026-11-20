//! Two promises of the server loop, held against a scripted backend.
//!
//! - A write the kernel refused only because the store moved under it
//!   (`retryable_conflict`) is planned and sent again by the server, and the
//!   caller hears the conflict only when every attempt lost (S1, N6 of the
//!   1.0 review). Any other refusal is answered at once.
//! - A request that panics is answered with a JSON-RPC error and the
//!   session goes on serving (B2).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use kmp_mcp::{KernelMcpServer, KernelMcpToolBackend, KernelMcpToolFuture, ToolError};
use serde_json::{Value, json};

enum Script {
    Replies(Mutex<VecDeque<Result<Value, ToolError>>>),
    Always(Result<Value, ToolError>),
    Panics,
}

struct ScriptedBackend {
    calls: Arc<Mutex<usize>>,
    script: Script,
}

impl KernelMcpToolBackend for ScriptedBackend {
    fn backend_name(&self) -> &'static str {
        "scripted"
    }

    fn grpc_tls_mode_name(&self) -> &'static str {
        "disabled"
    }

    fn call_tool<'a>(&'a self, _name: &'a str, _arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        *self.calls.lock().expect("calls") += 1;
        let reply = match &self.script {
            Script::Replies(replies) => replies
                .lock()
                .expect("replies")
                .pop_front()
                .expect("the script has a reply for every attempt"),
            Script::Always(reply) => reply.clone(),
            Script::Panics => panic!("scripted backend failure"),
        };
        Box::pin(async move { reply })
    }
}

fn server(script: Script) -> (KernelMcpServer, Arc<Mutex<usize>>) {
    let calls = Arc::new(Mutex::new(0));
    let backend = ScriptedBackend {
        calls: Arc::clone(&calls),
        script,
    };
    (
        KernelMcpServer::with_shared_backend(Arc::new(backend)),
        calls,
    )
}

fn ingest_call(id: u64) -> String {
    let arguments: Value = serde_json::from_str(include_str!(
        "../../../api/examples/kernel/v1beta1/kmp/ingest.request.json"
    ))
    .expect("ingest example");
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": "kmp_ingest", "arguments": arguments}})
    .to_string()
}

async fn send(server: &KernelMcpServer, line: &str) -> Value {
    let reply = server
        .handle_json_bytes(line.as_bytes())
        .await
        .expect("answered");
    serde_json::from_str(&reply).expect("JSON reply")
}

fn moved() -> ToolError {
    ToolError::retryable_conflict("the store moved while this write was prepared")
}

fn accepted() -> Value {
    json!({"content": [], "structuredContent": {"summary": "accepted"}, "isError": false})
}

#[tokio::test]
async fn a_write_that_lost_the_race_is_sent_again_until_it_lands() {
    let (server, calls) = server(Script::Replies(Mutex::new(VecDeque::from([
        Err(moved()),
        Err(moved()),
        Ok(accepted()),
    ]))));
    let reply = send(&server, &ingest_call(1)).await;
    assert_ne!(reply["result"]["isError"], true, "{reply}");
    assert_eq!(reply["result"]["structuredContent"]["summary"], "accepted");
    assert_eq!(*calls.lock().expect("calls"), 3);
}

#[tokio::test]
async fn the_caller_hears_the_retryable_code_only_when_every_attempt_lost() {
    let (server, calls) = server(Script::Always(Err(moved())));
    let reply = send(&server, &ingest_call(1)).await;
    assert_eq!(
        reply["result"]["structuredContent"]["error"]["code"], "retryable_conflict",
        "{reply}"
    );
    assert_eq!(*calls.lock().expect("calls"), 12, "bounded attempts");
}

#[tokio::test]
async fn a_conflict_that_must_not_be_replayed_is_answered_at_once() {
    let (server, calls) = server(Script::Always(Err(ToolError::conflict(
        "idempotency key already accepted with different content",
    ))));
    let reply = send(&server, &ingest_call(1)).await;
    assert_eq!(
        reply["result"]["structuredContent"]["error"]["code"], "conflict",
        "{reply}"
    );
    assert_eq!(*calls.lock().expect("calls"), 1);
}

#[tokio::test]
async fn a_request_that_panics_is_answered_and_the_session_goes_on() {
    let (server, _) = server(Script::Panics);
    let reply = send(&server, &ingest_call(7)).await;
    assert_eq!(reply["id"], 7, "{reply}");
    assert_eq!(reply["error"]["code"], -32603, "{reply}");
    assert!(
        reply["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("scripted backend failure")),
        "{reply}"
    );
    let listed = send(
        &server,
        &json!({"jsonrpc": "2.0", "id": 8, "method": "tools/list"}).to_string(),
    )
    .await;
    assert!(listed["result"]["tools"].is_array(), "{listed}");
}
