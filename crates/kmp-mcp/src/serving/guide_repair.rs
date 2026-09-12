use serde_json::{Value, json};

use super::{ToolError, ToolErrorCode};

/// Repair instructions for an unavailable installed guide, without seeding it.
pub(super) struct GuideRepair;

impl GuideRepair {
    pub(super) fn missing(node_ref: &str, error: ToolError) -> ToolError {
        if error.code == ToolErrorCode::NotFound {
            Self::unavailable(node_ref, error)
        } else {
            error
        }
    }

    pub(super) fn for_inspect(arguments: &Value, error: &ToolError) -> Option<ToolError> {
        let about = arguments["about"].as_str()?;
        let node_ref = arguments["ref"].as_str()?;
        (crate::guide::is_guide_about(about)
            && node_ref.starts_with(&format!("{about}:"))
            && error.code == ToolErrorCode::NotFound)
            .then(|| Self::unavailable(node_ref, error.clone()))
    }

    pub(super) fn unavailable(node_ref: &str, mut error: ToolError) -> ToolError {
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
        error.message = format!("{reason} {instructions} Cause: {}", error.message);
        error.with_feedback(json!({
            "code":"GUIDE_UNAVAILABLE", "field":"guide", "ref":node_ref, "reason":reason,
            "repair":{
                "command":"kmp-mcp",
                "arguments":["guide","sync","--plugin-root","<plugin-root>"],
                "instructions":instructions
            }
        }))
    }
}
