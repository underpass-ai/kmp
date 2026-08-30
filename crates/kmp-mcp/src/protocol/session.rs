//! The handshake: who this server says it is, and the names it answers to.
//!
//! One concept: everything a client learns before it calls a tool — the
//! protocol version, the server identity, the declared capabilities, and the
//! one-minor aliases that keep a saved permission rule working.

use serde_json::{Value, json};

use crate::protocol::chronoloom_app::MCP_APP_MIME;

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "underpass-kmp-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Resolves the one-minor compatibility aliases to the names advertised by
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

pub(crate) fn initialize_result_with_apps(backend: &str, grpc_tls: &str, apps: bool) -> Value {
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
        "instructions": crate::agent_policy::mcp_instructions(),
        "metadata": {
            "backend": backend,
            "grpc_tls": grpc_tls
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::registry::tools_list_result;

    #[test]
    fn former_tool_names_resolve_to_the_advertised_kmp_surface() {
        let former = [
            "kernel_ingest",
            "kernel_write_memory",
            "kernel_wake",
            "kernel_ask",
            "kernel_goto",
            "kernel_near",
            "kernel_rewind",
            "kernel_forward",
            "kernel_trace",
            "kernel_inspect",
        ];
        let tools = tools_list_result();
        let current = tools["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .collect::<Vec<_>>();

        // Every former name still resolves to something this surface
        // advertises. It is not an equality: the view tools were born with
        // their kmp_ names and never had a kernel_ one to rename.
        for name in former.map(canonical_tool_name) {
            assert!(
                current.contains(&name),
                "former name resolves to `{name}`, which the surface no longer advertises"
            );
        }
        assert!(current.iter().all(|name| name.starts_with("kmp_")));
    }

    #[test]
    fn initialize_result_reports_backend_metadata() {
        let result = initialize_result_with_apps("stub", "mutual", false);

        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], SERVER_NAME);
        assert_eq!(result["metadata"]["backend"], "stub");
        assert_eq!(result["metadata"]["grpc_tls"], "mutual");
        let instructions = result["instructions"].as_str().expect("instructions");
        assert!(instructions.contains("Temporal intent has precedence"));
        assert!(instructions.contains("Preserve evidence text"));
        assert!(instructions.contains("Refs are opaque identifiers"));
        assert!(instructions.contains("Never prefix or qualify it with an about"));
    }
}
