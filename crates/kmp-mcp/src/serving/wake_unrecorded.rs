//! A first wake on an about nothing was written to yet is the normal start of
//! new work, not a failure: it answers that nothing is recorded there, and
//! the store disclosure still says which store was read.

use serde_json::{Value, json};

use crate::serving::{ToolError, ToolErrorCode, tool_success_result};

/// The answer for a wake whose about is absent; `None` for any other error.
/// A page of an earlier wake, or a historical cut at a ref, may miss
/// something other than the about, so those keep their error.
pub(super) fn unrecorded_wake(tool: &str, arguments: &Value, error: &ToolError) -> Option<Value> {
    if tool != "kmp_wake" || error.code != ToolErrorCode::NotFound {
        return None;
    }
    let about = arguments.get("about")?.as_str()?;
    let narrowed = arguments.pointer("/page/cursor").is_some()
        || arguments.pointer("/as_of/ref").is_some()
        || crate::guide::is_guide_about(about);
    if narrowed {
        return None;
    }
    Some(tool_success_result(json!({
        "summary": format!("Nothing is recorded for `{about}` in this store yet.")
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_missing_about_on_a_first_wake_is_answered() {
        let missing = ToolError::not_found("node 'project:new' not found");
        let answer = unrecorded_wake("kmp_wake", &json!({"about":"project:new"}), &missing)
            .expect("answered");
        assert_eq!(answer["isError"], false);
        assert!(
            answer["structuredContent"]["summary"]
                .as_str()
                .is_some_and(|summary| summary.contains("project:new"))
        );
        for arguments in [
            json!({"about":"project:new","page":{"cursor":"c"}}),
            json!({"about":"project:new","as_of":{"ref":"x"}}),
            json!({"about":"guide:kmp-agent"}),
        ] {
            assert!(unrecorded_wake("kmp_wake", &arguments, &missing).is_none());
        }
        let about = json!({"about":"project:new"});
        assert!(unrecorded_wake("kmp_ask", &about, &missing).is_none());
        assert!(unrecorded_wake("kmp_wake", &about, &ToolError::backend("down")).is_none());
    }
}
