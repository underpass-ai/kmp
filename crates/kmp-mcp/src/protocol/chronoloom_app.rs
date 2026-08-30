//! The ChronoLoom MCP App resource.
//!
//! One concept: the single UI resource this server publishes — its URI, its
//! MIME profile, and the document a host reads to mount it. The URI is a
//! constant because three surfaces name it: the resource listing, the app-only
//! tools' `_meta`, and the app data results.

use serde_json::{Value, json};

use crate::tool_error::ToolError;

pub(crate) const CHRONOLOOM_APP_URI: &str = "ui://kmp/chronoloom.html";
pub(crate) const MCP_APP_MIME: &str = "text/html;profile=mcp-app";

pub(crate) fn resources_list_result() -> Value {
    json!({
        "resources": [{
            "uri": CHRONOLOOM_APP_URI,
            "name": "KMP ChronoLoom",
            "description": "Interactive polytemporal memory loom with proof-preserving navigation.",
            "mimeType": MCP_APP_MIME,
            "_meta": {"ui": {"prefersBorder": true}}
        }]
    })
}

pub(crate) fn resource_read_result(uri: &str) -> Result<Value, ToolError> {
    if uri != CHRONOLOOM_APP_URI {
        return Err(ToolError::not_found(format!(
            "unknown MCP resource `{uri}`"
        )));
    }
    Ok(json!({
        "contents": [{
            "uri": CHRONOLOOM_APP_URI,
            "mimeType": MCP_APP_MIME,
            "text": kmp_viewer::mcp_app_html(),
            "_meta": {
                "ui": {
                    "csp": {
                        "connectDomains": [],
                        "resourceDomains": [],
                        "frameDomains": [],
                        "baseUriDomains": []
                    },
                    "prefersBorder": true
                }
            }
        }]
    }))
}
