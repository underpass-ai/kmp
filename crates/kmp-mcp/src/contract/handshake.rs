use serde_json::{Value, json};

use crate::serving::ToolError;

pub(crate) const PROTOCOL_VERSION: &str = "2024-11-05";
pub(crate) const SERVER_NAME: &str = "underpass-kmp-mcp";
pub(crate) const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) const CHRONOLOOM_APP_URI: &str = "ui://kmp/chronoloom.html";
pub(crate) const MCP_APP_MIME: &str = "text/html;profile=mcp-app";

/// `tools/list`.
///
/// Alias resolution happens at the JSON-RPC boundary. Backends, telemetry and
/// newly written provenance therefore see one canonical vocabulary, while a
/// saved permission rule or script using the former names keeps working for
/// the migration release.
pub(crate) fn canonical_tool_name(name: &str) -> &str {
    match name {
        "kernel_ingest" => "kmp_ingest",
        "kernel_write_memory" => "kmp_write_memory",
        "kernel_wake" => "kmp_wake",
        "kernel_ask" => "kmp_ask",
        "kernel_goto" => "kmp_goto",
        "kernel_near" => "kmp_near",
        "kernel_rewind" => "kmp_rewind",
        "kernel_forward" => "kmp_forward",
        "kernel_trace" => "kmp_trace",
        "kernel_inspect" => "kmp_inspect",
        _ => name,
    }
}

#[cfg(test)]
pub(crate) fn initialize_result(backend: &str, grpc_tls: &str) -> Value {
    initialize_result_with_apps(backend, grpc_tls, false, false)
}

/// `bridges_languages` says whether the backend's `kmp_ask` crosses languages
/// on its own, which decides which language rule the agent is handed.
pub(crate) fn initialize_result_with_apps(
    backend: &str,
    grpc_tls: &str,
    apps: bool,
    bridges_languages: bool,
) -> Value {
    let mut capabilities = json!({"tools": {}});
    if apps {
        capabilities["resources"] = json!({});
        capabilities["extensions"] = json!({
            "io.modelcontextprotocol/ui": {"mimeTypes": [MCP_APP_MIME]}
        });
    }
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": capabilities,
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION
        },
        "instructions": crate::agent_policy::mcp_instructions(bridges_languages),
        "metadata": {
            "backend": backend,
            "grpc_tls": grpc_tls
        }
    })
}

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
