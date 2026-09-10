use serde_json::Value;

use super::GuidanceError;

/// A read or reviewed write at the MCP boundary, retained as transport metadata.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReadContinuation {
    pub(crate) tool: String,
    pub(crate) arguments: Value,
}

impl ReadContinuation {
    pub(crate) fn supports(tool: &str) -> bool {
        tool == "kmp_write_memory" || Self::supports_read(tool)
    }

    /// Read-only response projection must never classify a resumable write as a read.
    pub(crate) fn supports_read(tool: &str) -> bool {
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
            || (tool == "kmp_write_memory"
                && arguments
                    .get("review_token")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty))
        {
            return Err(GuidanceError::InvalidSession(
                "a continuation must contain one complete native read or a write with its neighborhood review token".into(),
            ));
        }
        Ok(Self {
            tool: tool.to_owned(),
            arguments,
        })
    }
}
