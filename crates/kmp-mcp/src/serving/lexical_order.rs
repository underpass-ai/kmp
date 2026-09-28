use serde_json::Value;

/// When a call's empty and repeated values are refused (#850). The catalogue
/// no longer advertises `minLength`/`uniqueItems`, so the server enforces them;
/// the question is only which refusal an agent sees first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LexicalOrder {
    /// Writes and the guide: nothing may commit on an argument the contract
    /// refuses, so the check runs before dispatch.
    BeforeDispatch,
    /// The writer's planner names field-level problems with feedback of its
    /// own and runs the check after planning, before commit.
    AfterPlanning,
    /// Reads have no side effect; their projections refuse some of the same
    /// values with a precise vocabulary, so the check backs them up afterwards.
    AfterRead,
}

impl LexicalOrder {
    pub(super) fn of(tool: &str) -> Self {
        match tool {
            "kmp_write_memory" => Self::AfterPlanning,
            "kmp_ingest" | "kmp_relabel" | "kmp_condense" | "kmp_curate" | "kmp_guide" => {
                Self::BeforeDispatch
            }
            _ => Self::AfterRead,
        }
    }
}

/// Whether a JSON-RPC `tools/call` reply already carries a refusal.
pub(super) fn is_tool_error(reply: &str) -> bool {
    serde_json::from_str::<Value>(reply).map_or(true, |reply| {
        reply.get("error").is_some() || reply["result"]["isError"] == Value::Bool(true)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_are_checked_before_dispatch_and_reads_after() {
        assert_eq!(LexicalOrder::of("kmp_ingest"), LexicalOrder::BeforeDispatch);
        assert_eq!(LexicalOrder::of("kmp_curate"), LexicalOrder::BeforeDispatch);
        assert_eq!(
            LexicalOrder::of("kmp_write_memory"),
            LexicalOrder::AfterPlanning
        );
        assert_eq!(LexicalOrder::of("kmp_time"), LexicalOrder::AfterRead);
        assert_eq!(LexicalOrder::of("kmp_wake"), LexicalOrder::AfterRead);
    }

    #[test]
    fn a_refused_or_unreadable_reply_counts_as_an_error() {
        assert!(is_tool_error(r#"{"result":{"isError":true}}"#));
        assert!(is_tool_error(r#"{"error":{"code":-32602}}"#));
        assert!(is_tool_error("not json"));
        assert!(!is_tool_error(r#"{"result":{"content":[]}}"#));
    }
}
