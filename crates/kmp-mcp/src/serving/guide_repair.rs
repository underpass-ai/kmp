use serde_json::{Value, json};

use super::{ToolError, ToolErrorCode};

/// Repair instructions for an unavailable installed guide, without seeding it.
pub(super) struct GuideRepair;

impl GuideRepair {
    pub(super) fn missing(node_ref: &str, error: ToolError, searched: &[String]) -> ToolError {
        if error.code == ToolErrorCode::NotFound {
            Self::unavailable_after(node_ref, error, searched)
        } else {
            error
        }
    }

    /// Whether `arguments` read a guide node and `error` is the not-found
    /// that an absent guide answers with.
    pub(super) fn is_missing_guide_read(arguments: &Value, error: &ToolError) -> bool {
        let (Some(about), Some(node_ref)) =
            (arguments["about"].as_str(), arguments["ref"].as_str())
        else {
            return false;
        };
        crate::guide::is_guide_about(about)
            && node_ref.starts_with(&format!("{about}:"))
            && error.code == ToolErrorCode::NotFound
            && !Self::already_explained(error)
    }

    pub(super) fn for_inspect(arguments: &Value, error: &ToolError) -> Option<ToolError> {
        let node_ref = arguments["ref"].as_str()?;
        Self::is_missing_guide_read(arguments, error)
            .then(|| Self::unavailable(node_ref, error.clone()))
    }

    fn already_explained(error: &ToolError) -> bool {
        error
            .feedback
            .iter()
            .any(|item| item["code"] == "GUIDE_UNAVAILABLE")
    }

    pub(super) fn unavailable(node_ref: &str, error: ToolError) -> ToolError {
        Self::unavailable_after(node_ref, error, &[])
    }

    /// The repair, naming where this session looked for installed assets
    /// before asking a person to type the path.
    pub(super) fn unavailable_after(
        node_ref: &str,
        mut error: ToolError,
        searched: &[String],
    ) -> ToolError {
        let reason = format!(
            "The selected store cannot serve guide node `{node_ref}`; its guide bundle may be \
             absent, incomplete or from another version. Check the ref and matching plugin assets."
        );
        let instructions = "To install or repair the guide explicitly, use the same binary, \
            working directory and store/backend environment as this MCP connection (including \
            KMP_MCP_DATA_DIR when set). Stop the MCP process first if it holds the embedded store. \
            Run `kmp-mcp guide sync --plugin-root <plugin-root>`, replacing <plugin-root> with the \
            matching plugin directory containing guide/guide.requests.json and guide/memory.jsonl. \
            Sync writes both guide abouts and may update the maintained memory bundle. Then \
            restart this connection with the same store selection and retry the original call. \
            Repeating that sync is idempotent; a runtime guide read does not install assets.";
        let looked = if searched.is_empty() {
            String::new()
        } else {
            format!(
                " This session looked for matching installed assets first: {}.",
                searched.join("; ")
            )
        };
        error.message = format!("{reason}{looked} {instructions} Cause: {}", error.message);
        error.with_feedback(json!({
            "code":"GUIDE_UNAVAILABLE", "field":"guide", "ref":node_ref, "reason":reason,
            "searched":searched,
            "repair":{
                "command":"kmp-mcp",
                "arguments":["guide","sync","--plugin-root","<plugin-root>"],
                "instructions":instructions
            }
        }))
    }
}
