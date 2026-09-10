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
        if !ReadContinuation::supports(name)
            || arguments.as_object().is_none_or(|args| args.len() != 1)
        {
            return Err(ToolError::invalid_argument(
                "Use continuation alone on its returned verb; other arguments start a different call.",
            ));
        }
        let id = ReadContinuationId::parse(value.as_str().ok_or_else(|| {
            ToolError::invalid_argument("continuation must be a returned call identifier")
        })?)
        .map_err(|e| guidance_error(&e))?;
        let missing = || {
            ToolError::conflict("This continuation expired, was evicted, or belongs to another store. Submit the original call again; preserve its idempotency key and keep earlier read pages partial.").with_feedback(json!({"code":"CONTINUATION_UNAVAILABLE","severity":"error","field":"continuation","reason":"Start the original read again; a handle is bounded transport state, not durable evidence.","action":null}))
        };
        let directory = self.guidance_directory(true).map_err(|error| {
            if error.code == super::ToolErrorCode::InvalidArgument {
                missing()
            } else {
                error
            }
        })?;
        let call = directory
            .load_read(&id)
            .map_err(|e| guidance_error(&e))?
            .ok_or_else(missing)?;
        if call.tool != name {
            return Err(ToolError::invalid_argument(
                "continuation belongs to a different verb; copy the returned action",
            ));
        }
        crate::contract::reject_unknown_arguments(name, &call.arguments)?;
        Ok(Some(call.arguments))
    }

    pub(super) fn shorten_read_actions(&self, call: &CallGuidance, response: String) -> String {
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
        if let Some(action) = body.pointer_mut("/projection/next_action") {
            failed |= self.shorten_read_action(call, action).is_err();
        }
        for action in body
            .get_mut("next_actions")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            failed |= self.shorten_read_action(call, action).is_err();
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
        call: &CallGuidance,
        action: &mut Value,
    ) -> Result<(), ToolError> {
        let Some(tool) = action["tool"]
            .as_str()
            .filter(|tool| ReadContinuation::supports(tool))
        else {
            return Ok(());
        };
        if !action["arguments"].is_object() {
            return Ok(());
        }
        let placeholder = json!({"tool":tool,"arguments":{"continuation":"read_00000000000000000000000000000000"}});
        if placeholder.to_string().len() > action.to_string().len() {
            return Ok(());
        }
        let bound = call.contextualize(action.clone());
        let retained = ReadContinuation::new(tool, bound["arguments"].clone())
            .map_err(|e| guidance_error(&e))?;
        let id = self
            .guidance_directory(true)?
            .save_read(&call.context.session, &retained)
            .map_err(|e| guidance_error(&e))?;
        *action = json!({"tool":tool,"arguments":{"continuation":id.as_str()}});
        Ok(())
    }
}
