//! Store read failures must not be reclassified as caller validation failures.
use std::sync::{Arc, Mutex};

use kmp_mcp::{
    KernelMcpServer, KernelMcpToolBackend, KernelMcpToolFuture, ToolError, ToolErrorCode,
};
use serde_json::{Value, json};

struct FailingRead {
    reply: Result<Value, ToolError>,
    calls: Arc<Mutex<Vec<String>>>,
}

impl KernelMcpToolBackend for FailingRead {
    fn backend_name(&self) -> &'static str {
        "failing-read"
    }
    fn call_tool<'a>(&'a self, name: &'a str, _arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        self.calls.lock().expect("calls").push(name.to_owned());
        let reply = self.reply.clone();
        Box::pin(async move { reply })
    }
}

#[tokio::test]
async fn summary_pre_read_preserves_the_backend_category_and_never_attempts_ingest() {
    for (code, reply) in [
        ToolErrorCode::Unavailable,
        ToolErrorCode::NotFound,
        ToolErrorCode::Conflict,
        ToolErrorCode::BackendError,
        ToolErrorCode::InvalidArgument,
    ]
    .into_iter()
    .map(|code| {
        (
            code,
            Err(ToolError::new(code, "the source could not be read")),
        )
    })
    .chain([(
        ToolErrorCode::BackendError,
        Ok(json!({"object":{"ref":"project:error-check:entry:source"},"raw":[]})),
    )]) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let server = KernelMcpServer::with_backend(Arc::new(FailingRead {
            reply,
            calls: calls.clone(),
        }));
        let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
            "name":"kmp_write_memory","arguments":{
                "about":"project:error-check","actor":"writer","observed_at":"2026-09-01T09:00:00Z",
                "search_summaries":[{"ref":"project:error-check:entry:source","summary_en":"The cache stores a stable source."}]
            }
        }});
        let result = server
            .handle_json_line(&request.to_string())
            .await
            .expect("reply");
        let result: Value = serde_json::from_str(&result).expect("JSON reply");
        assert_eq!(result["result"]["isError"], true);
        assert_eq!(
            result["result"]["structuredContent"]["error"]["code"],
            code.as_str(),
            "{result}"
        );
        assert_eq!(*calls.lock().expect("calls"), ["kmp_inspect"]);
    }
}
