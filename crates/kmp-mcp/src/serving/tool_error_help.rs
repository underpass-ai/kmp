use serde_json::{Value, json};

use super::{ToolError, ToolErrorCode};

/// On-demand lessons for a refused call. They teach usage, never repair data.
pub(crate) struct ToolErrorHelp;

impl ToolErrorHelp {
    pub(super) fn topic(tool: &str) -> Option<&'static str> {
        Self::route(tool)?.0.strip_prefix("verb:")
    }

    pub(crate) fn for_call(tool: &str, arguments: &Value, error: &ToolError) -> Option<Value> {
        // A failed guide read must not recommend itself. Infrastructure failures
        // are not fixed by changing source data or repeatedly reading this store.
        if matches!(
            error.code,
            ToolErrorCode::BackendError | ToolErrorCode::Unavailable
        ) || arguments.get("about").and_then(Value::as_str) == Some("guide:kmp-agent")
        {
            return None;
        }
        let (verb, default_example) = Self::route(tool)?;
        let mut examples = Vec::new();
        for feedback in &error.feedback {
            if let Some(example) = Self::for_feedback(feedback)
                && !examples.contains(&example)
            {
                examples.push(example);
            }
        }
        if examples.is_empty() {
            examples.push(default_example);
        }
        Some(json!({
            "guide": Self::read(verb),
            "examples": examples.into_iter().map(Self::read).collect::<Vec<_>>()
        }))
    }

    fn for_feedback(feedback: &Value) -> Option<&'static str> {
        match feedback["code"].as_str().unwrap_or_default() {
            "FUTURE_OBSERVATION" => return Some("example:four-clocks"),
            "INVALID_LABELS" => return Some("example:dimensional-memberships"),
            "MEMORY_EVIDENCE_REQUIRED" | "INVALID_KIND" => return Some("example:first-decision"),
            "RELATE_PROPOSAL_REQUIRED" | "CROSS_ABOUT_RELATION" => {
                return Some("example:distributed-incident");
            }
            "RELATION_PROOF_REQUIRED"
            | "RELATION_CLASS_MISMATCH"
            | "PRIOR_CONTEXT_REQUIRED"
            | "INVALID_RELATION"
            | "SELF_RELATION" => return Some("example:decision-history"),
            "SEARCH_SUMMARY_REQUIRED"
            | "INVALID_SEARCH_SUMMARY"
            | "INVALID_LOCAL_ID"
            | "DUPLICATE_LOCAL_ID"
            | "UNKNOWN_LOCAL_REF"
            | "WRITE_OPERATION_REQUIRED"
            | "EMPTY_MEMORIES" => return Some("example:semantic-batch"),
            _ => {}
        }
        // Read validator-produced argument paths, never words in its message.
        let field = feedback["field"].as_str().unwrap_or_default();
        let segments = field.split(['.', '[']);
        for segment in segments {
            match segment {
                "occurred_at" | "observed_at" | "valid_from" | "valid_until" | "axis"
                | "interval" | "as_of" => return Some("example:four-clocks"),
                "page" | "budget" => return Some("example:budget-proof"),
                "labels" | "dimensions" => return Some("example:dimensional-memberships"),
                _ => {}
            }
        }
        None
    }

    fn route(tool: &str) -> Option<(&'static str, &'static str)> {
        Some(match tool {
            "kmp_guide" => ("verb:guide", "card:guide"),
            "kmp_write_memory" => ("verb:write", "example:semantic-batch"),
            "kmp_relabel" => ("verb:write", "example:dimensional-memberships"),
            "kmp_ingest" => ("verb:write", "example:canonical-ingest"),
            "kmp_wake" => ("verb:wake", "example:shared-resumption"),
            "kmp_ask" => ("verb:ask", "example:decision-history"),
            "kmp_relate" => ("verb:relate", "example:distributed-incident"),
            "kmp_goto" | "kmp_near" | "kmp_rewind" | "kmp_forward" => {
                ("verb:time", "example:four-clocks")
            }
            "kmp_inspect" => ("verb:audit", "example:budget-proof"),
            "kmp_trace" => ("verb:audit", "example:decision-history"),
            "kmp_view_open" | "kmp_view_get_state" | "kmp_view_apply_intent" => {
                ("verb:view", "example:shared-resumption")
            }
            _ => return None,
        })
    }

    fn read(suffix: &str) -> Value {
        json!({"tool":"kmp_inspect", "arguments":{
            "about":"guide:kmp-agent", "ref":format!("guide:kmp-agent:{suffix}"),
            "include":{"details":false,"incoming":false,"outgoing":false,"raw":false},
            "budget":{"max_bytes":10000}
        }})
    }
}
