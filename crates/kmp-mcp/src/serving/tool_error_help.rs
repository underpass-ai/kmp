use serde_json::Value;

use super::{ToolError, ToolErrorCode};
use crate::guide::GuideTopic;

/// The guide topic a refused call points at: one short line, never a lesson
/// body. It teaches usage, never repairs data.
pub(crate) struct ToolErrorHelp;

impl ToolErrorHelp {
    /// `Help: kmp_guide {"topic":"…"}` for a refusal the guide can teach.
    pub(crate) fn line(tool: &str, arguments: &Value, error: &ToolError) -> Option<String> {
        Self::topic(tool, arguments, error)
            .map(|topic| format!("Help: kmp_guide {{\"topic\":\"{topic}\"}}"))
    }

    fn topic(tool: &str, arguments: &Value, error: &ToolError) -> Option<&'static str> {
        // A failed guide read must not recommend itself. Infrastructure failures
        // are not fixed by changing source data or repeatedly reading this store.
        if tool == "kmp_guide"
            || matches!(
                error.code,
                ToolErrorCode::BackendError | ToolErrorCode::Unavailable
            )
            || arguments
                .get("about")
                .and_then(Value::as_str)
                .is_some_and(crate::guide::is_guide_about)
            || error
                .feedback
                .iter()
                .any(|item| item["code"] == "GUIDE_UNAVAILABLE")
        {
            return None;
        }
        // The topic that teaches the first refused argument, read from the
        // validator's typed path, never from words in its message.
        error
            .feedback
            .iter()
            .filter(|item| item["severity"] != "warning")
            .filter_map(|item| item["field"].as_str())
            .filter(|field| !field.is_empty())
            .find_map(|field| GuideTopic::explaining(tool, field))
            .or_else(|| GuideTopic::of_tool(tool))
            .map(|topic| topic.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_refusal_points_at_the_topic_of_its_first_refused_argument() {
        let relation = ToolError::invalid_argument("bad").with_feedback(
            json!({"code":"INVALID_RELATION","severity":"error","field":"memories[0].connect_to[1].rel"}),
        );
        assert_eq!(
            ToolErrorHelp::line("kmp_write_memory", &json!({}), &relation).as_deref(),
            Some(r#"Help: kmp_guide {"topic":"relations"}"#)
        );
        let plain = ToolError::invalid_argument("bad");
        assert_eq!(
            ToolErrorHelp::line("kmp_write_memory", &json!({}), &plain).as_deref(),
            Some(r#"Help: kmp_guide {"topic":"write"}"#)
        );
        let seek = ToolError::invalid_argument("bad")
            .with_feedback(json!({"severity":"error","field":"search.seek[0].via"}));
        assert_eq!(
            ToolErrorHelp::line("kmp_trace", &json!({}), &seek).as_deref(),
            Some(r#"Help: kmp_guide {"topic":"trace"}"#)
        );
    }

    #[test]
    fn guide_reads_infrastructure_and_unknown_tools_get_no_pointer() {
        let bad = ToolError::invalid_argument("bad");
        assert!(ToolErrorHelp::line("kmp_guide", &json!({}), &bad).is_none());
        assert!(ToolErrorHelp::line("kmp_unknown", &json!({}), &bad).is_none());
        let guide = json!({"about":"guide:kmp-agent"});
        assert!(ToolErrorHelp::line("kmp_inspect", &guide, &bad).is_none());
        let down = ToolError::backend("down");
        assert!(ToolErrorHelp::line("kmp_ask", &json!({}), &down).is_none());
    }
}
