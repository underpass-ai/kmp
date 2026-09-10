use serde_json::Value;

use super::GuidanceError;

/// A read call at the MCP boundary, retained as transport metadata, not evidence.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReadContinuation {
    pub(crate) tool: String,
    pub(crate) arguments: Value,
}

impl ReadContinuation {
    pub(crate) fn supports(tool: &str) -> bool {
        matches!(
            tool,
            "kmp_wake"
                | "kmp_ask"
                | "kmp_inspect"
                | "kmp_trace"
                | "kmp_relate"
                | "kmp_goto"
                | "kmp_near"
                | "kmp_rewind"
                | "kmp_forward"
        )
    }

    pub(crate) fn new(tool: &str, arguments: Value) -> Result<Self, GuidanceError> {
        if !Self::supports(tool)
            || !arguments.is_object()
            || arguments.get("continuation").is_some()
        {
            return Err(GuidanceError::InvalidSession(
                "a continuation must contain one complete native memory read".into(),
            ));
        }
        Ok(Self {
            tool: tool.to_owned(),
            arguments,
        })
    }
}
