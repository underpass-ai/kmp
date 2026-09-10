use super::guide_dispatch::guidance_error;
use super::{KernelMcpServer, ToolError};
use crate::guidance::{AgentContext, AgentContextId, GuidancePurpose};
use serde_json::{Value, json};

/// Optional protocol context. It is removed before a memory operation is compiled.
pub(super) struct CallGuidance {
    pub(super) context: AgentContext,
    pub(super) purpose: GuidancePurpose,
}

impl CallGuidance {
    pub(super) fn prepare(
        server: &KernelMcpServer,
        arguments: &Value,
    ) -> Result<Option<Self>, ToolError> {
        let Some(value) = arguments.get("context_id") else {
            if arguments.get("purpose").is_some() {
                return Err(ToolError::invalid_argument(
                    "purpose requires the context_id returned by kmp_guide",
                ));
            }
            return Ok(None);
        };
        let id = AgentContextId::parse(value.as_str().ok_or_else(|| {
            ToolError::invalid_argument("context_id must be the exact string returned by kmp_guide")
        })?)
        .map_err(|e| guidance_error(&e))?;
        let purpose = match arguments.get("purpose") {
            None => GuidancePurpose::Continue,
            Some(Value::String(value)) => match value.as_str() {
                "continue" => GuidancePurpose::Continue,
                "audit" => GuidancePurpose::Audit,
                "history" => GuidancePurpose::History,
                "answer" => GuidancePurpose::Answer,
                _ => {
                    return Err(ToolError::invalid_argument(
                        "purpose must be continue, audit, history or answer",
                    ));
                }
            },
            _ => return Err(ToolError::invalid_argument("purpose must be a string")),
        };
        let context = server
            .guidance_directory(true)?
            .context(&id)
            .map_err(|e| guidance_error(&e))?;
        Ok(Some(Self { context, purpose }))
    }

    pub(super) fn memory_arguments(&self, tool: &str, original: &Value) -> Value {
        let mut args = original.clone();
        if let Some(object) = args.as_object_mut() {
            object.remove("context_id");
            object.remove("purpose");
            if matches!(tool, "kmp_write_memory" | "kmp_relabel") && !object.contains_key("actor") {
                object.insert("actor".into(), json!(self.context.identity.name));
            }
        }
        args
    }

    pub(super) fn contextualize(&self, mut action: Value) -> Value {
        if let Some(arguments) = action.get_mut("arguments").and_then(Value::as_object_mut) {
            arguments.insert(
                "context_id".into(),
                json!(self.context.session.context_id.as_str()),
            );
            if action["tool"] != "kmp_guide" {
                action["arguments"]["purpose"] = json!(self.purpose.as_str());
            }
        }
        action
    }
}
