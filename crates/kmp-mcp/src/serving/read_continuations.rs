use serde_json::{Value, json};

use super::call_guidance::CallGuidance;
use super::guide_dispatch::guidance_error;
use super::{KernelMcpServer, ToolError};
use crate::guidance::{ReadContinuation, ReadContinuationId};

impl KernelMcpServer {
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
        let repeat_core = continuation_repeat_core(name, arguments)?;
        let id = ReadContinuationId::parse(value.as_str().ok_or_else(|| {
            ToolError::invalid_argument("continuation must be a returned call identifier")
        })?)
        .map_err(|e| guidance_error(&e))?;
        let missing = || {
            ToolError::conflict("This continuation expired, was evicted, or belongs to another store. Submit the original call again; preserve its idempotency key and keep earlier read pages partial.").with_feedback(json!({"code":"CONTINUATION_UNAVAILABLE","severity":"error","field":"continuation","reason":"Start the original read again; a handle is bounded transport state, not durable evidence.","action":null}))
        };
        // Both kinds of handle live beside the store in the agent directory:
        // one proposed inside a guidance context, one proposed without.
        let directory = self.guidance_directory(true).map_err(|error| {
            if error.code == super::ToolErrorCode::InvalidArgument {
                missing()
            } else {
                error
            }
        })?;
        let call = match directory.load_open(&id).map_err(|e| guidance_error(&e))? {
            Some(call) => call,
            None => directory
                .load_read(&id)
                .map_err(|e| guidance_error(&e))?
                .ok_or_else(missing)?,
        };
        if call.tool != name {
            return Err(ToolError::invalid_argument(
                "continuation belongs to a different verb; copy the returned action",
            ));
        }
        crate::contract::reject_unknown_arguments(name, &call.arguments)?;
        let mut arguments = call.arguments;
        if repeat_core {
            // Rehydrate once: the page it returns proposes incremental pages again.
            if !arguments["page"].is_object() {
                arguments["page"] = json!({});
            }
            arguments["page"]["repeat_core"] = json!(true);
        }
        Ok(Some(arguments))
    }

    /// Replace each proposed call with a short handle to it. Inside a
    /// guidance context every returned read or reviewed write is retained
    /// for that context; without one, only continuations — a cursor page or
    /// a write resuming its review — are retained, so a page never
    /// restates the request while an alternative the agent must choose
    /// between stays readable (#544 C3).
    pub(super) fn shorten_read_actions(
        &self,
        call: Option<&CallGuidance>,
        response: String,
    ) -> String {
        if call.is_none() && !(response.contains("\"cursor\"") || response.contains("review_token"))
        {
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
            match self.shorten_read_action(call, action) {
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
            match self.shorten_read_action(call, action) {
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

    fn shorten_read_action(
        &self,
        call: Option<&CallGuidance>,
        action: &mut Value,
    ) -> Result<bool, ToolError> {
        let Some(tool) = action["tool"]
            .as_str()
            .filter(|tool| ReadContinuation::supports(tool))
        else {
            return Ok(false);
        };
        if !action["arguments"].is_object()
            || (call.is_none() && !continues(tool, &action["arguments"]))
        {
            return Ok(false);
        }
        let placeholder = json!({"tool":tool,"arguments":{"continuation":"read_00000000000000000000000000000000"}});
        if placeholder.to_string().len() > action.to_string().len() {
            return Ok(false);
        }
        let id = match call {
            Some(call) => {
                let bound = call.contextualize(action.clone());
                let retained = ReadContinuation::new(tool, bound["arguments"].clone())
                    .map_err(|e| guidance_error(&e))?;
                self.guidance_directory(true)?
                    .save_read(&call.context.session, &retained)
                    .map_err(|e| guidance_error(&e))?
            }
            None => {
                let retained = ReadContinuation::new(tool, action["arguments"].clone())
                    .map_err(|e| guidance_error(&e))?;
                // Beside the store when there is one, so a handle outlives a
                // restarted server; in memory for a server without a path.
                self.guidance_directory(false)?
                    .save_open(&retained)
                    .map_err(|e| guidance_error(&e))?
            }
        };
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

/// A handle stands for the whole call. The only flag it accepts beside it is
/// Wake/Ask's `page.repeat_core`, which resends the core of that same page.
fn continuation_repeat_core(tool: &str, arguments: &Value) -> Result<bool, ToolError> {
    let alone = || {
        ToolError::invalid_argument(
            "Use continuation alone on its returned verb (Wake and Ask also accept page.repeat_core); other arguments start a different call — send the original arguments with page.cursor instead.",
        )
    };
    let object = arguments.as_object().ok_or_else(alone)?;
    let Some(page) = object.get("page") else {
        return if object.len() == 1 {
            Ok(false)
        } else {
            Err(alone())
        };
    };
    let flags = page.as_object().ok_or_else(alone)?;
    if object.len() != 2
        || !matches!(tool, "kmp_wake" | "kmp_ask")
        || flags.len() != 1
        || !flags.contains_key("repeat_core")
    {
        return Err(alone());
    }
    flags["repeat_core"]
        .as_bool()
        .ok_or_else(|| ToolError::invalid_argument("page.repeat_core must be a boolean"))
}
