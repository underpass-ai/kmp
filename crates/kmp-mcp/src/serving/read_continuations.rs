use serde_json::{Map, Value, json};

use super::{KernelMcpServer, ToolError};
use crate::continuation::{
    ContinuationError, ContinuationStore, ReadContinuation, ReadContinuationId,
    SqliteContinuationStore,
};

fn continuation_error(error: &ContinuationError) -> ToolError {
    match error {
        ContinuationError::Invalid(message) => ToolError::invalid_argument(message),
        ContinuationError::Unavailable(message) => ToolError::backend(message),
    }
}

/// Arguments a caller may resend beside a handle: the call already binds
/// them, so the same value is accepted and a different one is refused.
const BOUND_ALONGSIDE: [&str; 2] = ["about", "actor"];

impl KernelMcpServer {
    /// The store of retained continuations. `require_existing` resolves a
    /// handle: a store that was never written holds none, and is not created
    /// just to say so.
    fn continuation_store(
        &self,
        require_existing: bool,
    ) -> Result<Option<&dyn ContinuationStore>, ToolError> {
        if self.continuation_store.get().is_none() {
            let store = match &self.continuation_store_path {
                Some(path) if !require_existing || path.is_file() => {
                    SqliteContinuationStore::at(path)
                }
                None if !require_existing => SqliteContinuationStore::volatile(),
                _ => return Ok(None),
            }
            .map_err(|e| continuation_error(&e))?;
            // Cache only successful opens. A transient storage failure must be retryable.
            let _ = self
                .continuation_store
                .set(std::sync::Arc::new(store) as std::sync::Arc<dyn ContinuationStore>);
        }
        Ok(self.continuation_store.get().map(AsRef::as_ref))
    }

    /// Resolve before transport authorization, then authorize and dispatch this
    /// exact returned request. A continuation is not a grant of access.
    pub fn resolve_read_request(&self, mut request: Value) -> Result<Value, ToolError> {
        if request["method"] != "tools/call" {
            return Ok(request);
        }
        let name = crate::contract::canonical_tool_name(
            request["params"]["name"].as_str().unwrap_or_default(),
        );
        if let Some(arguments) =
            self.resolve_read_arguments(name, &request["params"]["arguments"])?
        {
            request["params"]["arguments"] = arguments;
        }
        Ok(request)
    }

    pub(super) fn resolve_read_arguments(
        &self,
        name: &str,
        arguments: &Value,
    ) -> Result<Option<Value>, ToolError> {
        let Some(value) = arguments.get("continuation") else {
            return Ok(None);
        };
        if !ReadContinuation::supports(name) {
            return Err(ToolError::invalid_argument(
                "Use continuation alone on its returned verb; other arguments start a different call.",
            ));
        }
        let alongside = Alongside::read(name, arguments)?;
        let id = ReadContinuationId::parse(value.as_str().ok_or_else(|| {
            ToolError::invalid_argument("continuation must be a returned call identifier")
        })?)
        .map_err(|e| continuation_error(&e))?;
        let missing = || {
            ToolError::conflict("This continuation expired, was evicted, or belongs to another store. Submit the original call again; preserve its idempotency key and keep earlier read pages partial.").with_feedback(json!({"code":"CONTINUATION_UNAVAILABLE","severity":"error","field":"continuation","reason":"Start the original read again; a handle is bounded transport state, not durable evidence.","action":null}))
        };
        let call = match self.continuation_store(true)? {
            Some(store) => store.load(&id).map_err(|e| continuation_error(&e))?,
            None => None,
        }
        .ok_or_else(missing)?;
        if call.tool != name {
            return Err(ToolError::invalid_argument(
                "continuation belongs to a different verb; copy the returned action",
            ));
        }
        crate::contract::reject_unknown_arguments(name, &call.arguments)?;
        alongside.check_bound(&call.arguments)?;
        let mut arguments = call.arguments;
        if alongside.repeat_core {
            // Rehydrate once: the page it returns proposes incremental pages again.
            if !arguments["page"].is_object() {
                arguments["page"] = json!({});
            }
            arguments["page"]["repeat_core"] = json!(true);
        }
        Ok(Some(arguments))
    }

    /// Replace each proposed continuation — a cursor page or a write resuming
    /// its review — with a short handle to it, so a page never restates the
    /// request. A restart or a navigation the agent must choose between stays
    /// readable (#544 C3).
    pub(super) fn shorten_read_actions(&self, response: String) -> String {
        if !(response.contains("\"cursor\"") || response.contains("review_token")) {
            return response;
        }
        let Ok(mut envelope) = serde_json::from_str::<Value>(&response) else {
            return response;
        };
        let Some(result) = envelope.get_mut("result") else {
            return response;
        };
        if result["isError"] == true {
            return response;
        }
        let Some(body) = result.get_mut("structuredContent") else {
            return response;
        };
        let mut failed = false;
        let mut shortened = false;
        if let Some(action) = body.pointer_mut("/projection/next_action") {
            match self.shorten_read_action(action) {
                Ok(changed) => shortened |= changed,
                Err(_) => failed = true,
            }
        }
        for action in body
            .get_mut("next_actions")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            match self.shorten_read_action(action) {
                Ok(changed) => shortened |= changed,
                Err(_) => failed = true,
            }
        }
        if !shortened && !failed {
            return response;
        }
        // Only replace an action if the replacement is no larger. The memory
        // packet's byte ceiling, evidence and native cursor remain untouched.
        // Recall reports its used size; recompute its fixed point after shrinking.
        if body.pointer("/projection/budget/used_bytes").is_some() {
            loop {
                let bytes = body.to_string().len();
                if body["projection"]["budget"]["used_bytes"] == bytes {
                    break;
                }
                body["projection"]["budget"]["used_bytes"] = json!(bytes);
            }
        }
        if failed && let Some(content) = result["content"].as_array_mut() {
            content.push(json!({"type":"text","text":"Continuation metadata could not be retained. The memory read succeeded; use its returned complete action."}));
        }
        envelope.to_string()
    }

    fn shorten_read_action(&self, action: &mut Value) -> Result<bool, ToolError> {
        let Some(tool) = action["tool"]
            .as_str()
            .filter(|tool| ReadContinuation::supports(tool))
        else {
            return Ok(false);
        };
        if !action["arguments"].is_object() || !continues(tool, &action["arguments"]) {
            return Ok(false);
        }
        let placeholder = json!({"tool":tool,"arguments":{"continuation":"read_00000000000000000000000000000000"}});
        if placeholder.to_string().len() > action.to_string().len() {
            return Ok(false);
        }
        let retained = ReadContinuation::new(tool, action["arguments"].clone())
            .map_err(|e| continuation_error(&e))?;
        // Beside the store when there is one, so a handle outlives a
        // restarted server; in memory for a server without a path.
        let id = self
            .continuation_store(false)?
            .ok_or_else(|| ToolError::backend("continuation store unavailable"))?
            .save(&retained)
            .map_err(|e| continuation_error(&e))?;
        *action = json!({"tool":tool,"arguments":{"continuation":id.as_str()}});
        Ok(true)
    }
}

/// A call that continues the same packet: a cursor page, or a write
/// resuming the review it was answered with. Restarts and navigation are
/// choices, not continuations, and stay readable.
fn continues(tool: &str, arguments: &Value) -> bool {
    let non_empty = |pointer: &str| {
        arguments
            .pointer(pointer)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty())
    };
    if tool == "kmp_write_memory" {
        non_empty("/review_token")
    } else {
        non_empty("/page/cursor")
    }
}

/// What a caller sent beside a handle. A handle stands for the whole call:
/// Wake/Ask also accept `page.repeat_core`, which resends the core of that
/// same page, and any verb accepts an argument the call already binds when
/// it carries the same value.
struct Alongside {
    repeat_core: bool,
    bound: Map<String, Value>,
}

impl Alongside {
    fn read(tool: &str, arguments: &Value) -> Result<Self, ToolError> {
        let alone = || {
            ToolError::invalid_argument(
                "Use continuation alone on its returned verb (Wake and Ask also accept page.repeat_core); other arguments start a different call — send the original arguments with page.cursor instead.",
            )
        };
        let object = arguments.as_object().ok_or_else(alone)?;
        let mut repeat_core = false;
        let mut bound = Map::new();
        for (key, value) in object {
            match key.as_str() {
                "continuation" => {}
                "page" if matches!(tool, "kmp_wake" | "kmp_ask") => {
                    let flags = value.as_object().ok_or_else(alone)?;
                    if flags.len() != 1 || !flags.contains_key("repeat_core") {
                        return Err(alone());
                    }
                    repeat_core = flags["repeat_core"].as_bool().ok_or_else(|| {
                        ToolError::invalid_argument("page.repeat_core must be a boolean")
                    })?;
                }
                key if BOUND_ALONGSIDE.contains(&key) => {
                    bound.insert(key.to_string(), value.clone());
                }
                _ => return Err(alone()),
            }
        }
        Ok(Self { repeat_core, bound })
    }

    /// Every argument resent beside the handle must equal the one the call
    /// was retained with; a different value would be a different call.
    fn check_bound(&self, retained: &Value) -> Result<(), ToolError> {
        for (key, sent) in &self.bound {
            match retained.get(key) {
                Some(kept) if kept == sent => {}
                Some(kept) => {
                    return Err(ToolError::invalid_argument(format!(
                        "this continuation is bound to {key} {kept}, not {sent}: drop {key} to continue, or send the original call to start another"
                    )));
                }
                None => {
                    return Err(ToolError::invalid_argument(format!(
                        "this continuation carries no {key}: drop {key} to continue, or send the original call to start another"
                    )));
                }
            }
        }
        Ok(())
    }
}
