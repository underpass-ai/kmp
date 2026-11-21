//! How a `kmp_write_memory` call the schema refused is worded.
//!
//! The refusal itself is final: the schema decided it at the boundary and
//! nothing can commit. The writer's planner names field-level problems
//! better than a schema can — every refused member at once, the feedback
//! codes the guide teaches, the label keys the about already holds — so when
//! the planner refuses the same packet, its refusal is the one returned. The
//! planner is a pure compiler: it reads the arguments and writes nothing.
//! The search-summary shape plans against the store, so there the schema's
//! own words are returned.

use serde_json::Value;

use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::serving::tool_error::ToolError;
use crate::write::validation_error::WriteValidationError;
use crate::write::{build_batch_plan, build_relation_plan};

impl KernelMcpServer {
    pub(super) async fn word_write_refusal(
        &self,
        arguments: &Value,
        refused: ToolError,
    ) -> ToolError {
        // An argument the writer does not have is named as such: the
        // planner never sees it, and would refuse something else.
        if refused
            .feedback
            .iter()
            .any(|item| item["code"] == "UNKNOWN_ARGUMENT")
        {
            return refused;
        }
        let planner = match (
            arguments.get("memories"),
            arguments.get("search_summaries"),
            arguments.get("relations"),
        ) {
            (Some(_), None, None) => match build_batch_plan(arguments) {
                Ok(_) => None,
                Err(mut errors) => {
                    if errors.concern_labels()
                        && let Some(about) = arguments.get("about").and_then(Value::as_str)
                        && let Some(keys) = self.about_label_keys(about).await
                    {
                        errors.name_label_keys(&keys);
                    }
                    Some(ToolError::from(errors))
                }
            },
            (None, None, Some(_)) => build_relation_plan(arguments).err().map(ToolError::from),
            (None, Some(_), None) => None,
            _ if arguments.get("continuation").is_some() => None,
            _ => Some(
                WriteValidationError::new(
                    "provide exactly one of memories, search_summaries or relations",
                )
                .code("WRITE_OPERATION_REQUIRED")
                .into(),
            ),
        };
        // The planner words the refusal in the writer's own codes; the
        // cross-field rule the schema found broken stays named beside them.
        let rule = refused
            .feedback
            .iter()
            .find_map(|item| item.get("rule").cloned());
        let mut error = planner.unwrap_or(refused);
        if let (Some(rule), Some(first)) = (rule, error.feedback.first_mut()) {
            first["rule"] = rule;
        }
        error
    }
}
