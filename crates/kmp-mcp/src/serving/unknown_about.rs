//! An about the store has never held, named as such.
//!
//! The kernel is node-centric: asked to read an about it does not hold, it
//! answers "node 'x' not found", or that a ref "does not belong to about"
//! the caller named. Neither is the problem the caller can fix. On the error
//! path only (a successful read pays nothing), the server asks whether the
//! about exists and, when it does not, says so — one code and one message
//! for every read rooted at an about (E7, E15).

use serde_json::Value;

use super::KernelMcpServer;
use crate::serving::tool_error::ToolError;
use crate::serving::tool_error_code::ToolErrorCode;

/// The reads whose root is the `about` argument. `kmp_wake` is not here: an
/// about with nothing recorded wakes to an empty start on purpose.
const ROOTED_AT_ABOUT: &[&str] = &[
    "kmp_ask",
    "kmp_relate",
    "kmp_time",
    "kmp_trace",
    "kmp_inspect",
    "kmp_curate",
];

impl KernelMcpServer {
    pub(super) async fn name_unknown_about(
        &self,
        tool: &str,
        arguments: &Value,
        error: ToolError,
    ) -> ToolError {
        // A handle or a review token names its own read; its refusal is about
        // the handle, not the about.
        let handled = ["continuation", "review_token"]
            .iter()
            .any(|key| arguments.get(*key).is_some());
        // The local product only: a remote kernel keeps its own refusals,
        // and the next steps below name this machine's store.
        if self.backend_name() != "embedded"
            || !ROOTED_AT_ABOUT.contains(&tool)
            || handled
            || matches!(
                error.code,
                ToolErrorCode::Unavailable | ToolErrorCode::RetryableConflict
            )
        {
            return error;
        }
        // A guide about that is missing has its own repair (guide_repair).
        let Some(about) = arguments
            .get("about")
            .and_then(Value::as_str)
            .filter(|about| !about.starts_with("guide:"))
        else {
            return error;
        };
        match self.memory_ref_exists(about, about).await {
            Ok(false) => ToolError::not_found(unknown_about_message(about)),
            _ => error,
        }
    }
}

/// `kmp_view_open`'s refusal of an about the store does not hold: the same
/// words, and why a view cannot degrade the way an intent does.
pub(super) fn unknown_view_about(about: &str) -> ToolError {
    ToolError::not_found(format!(
        "{} A view onto it would render an empty loom that looks like an answer.",
        unknown_about_message(about)
    ))
}

/// The refusal for an about this store does not hold, with the next steps
/// that work.
pub(super) fn unknown_about_message(about: &str) -> String {
    format!(
        "`{about}` is not an about in this store: nothing was ever written under it. Check the \
         name (an about is written exactly as a read or a write returned it; `kmp-mcp memories` \
         lists the stores on this machine), or write its first memory with kmp_write_memory."
    )
}

#[cfg(test)]
mod tests {
    use super::unknown_about_message;

    #[test]
    fn the_refusal_names_the_about_and_never_a_node() {
        let message = unknown_about_message("proj:nope");
        assert!(message.contains("`proj:nope` is not an about"), "{message}");
        assert!(!message.contains("node"), "{message}");
        assert!(message.contains("kmp_write_memory"), "{message}");
    }
}
