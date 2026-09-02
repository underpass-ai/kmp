//! JSON-RPC method routing: one newline-delimited message in, at most one
//! response out. Tool calls validate before anything reads the arguments.

use std::sync::atomic::Ordering;
use std::time::Instant;

use serde_json::Value;

use crate::contract::{
    canonical_tool_name, initialize_result_with_apps, reject_unknown_arguments,
    resource_read_result, resources_list_result, tools_list_result_with_apps,
};
use crate::serving::json_rpc::{jsonrpc_error, jsonrpc_result};
use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::serving::telemetry::{ToolErrorKind, record_tool_error, record_tool_success};
use crate::serving::tool_error::ToolError;
use crate::serving::tool_result::tool_error_result;

fn client_supports_apps(request: &Value) -> bool {
    request
        .pointer("/params/capabilities/extensions/io.modelcontextprotocol~1ui/mimeTypes")
        .and_then(Value::as_array)
        .is_some_and(|types| {
            types
                .iter()
                .any(|value| value.as_str() == Some(crate::contract::MCP_APP_MIME))
        })
}

impl KernelMcpServer {
    pub async fn handle_json_line(&self, line: &str) -> Option<String> {
        let request = match serde_json::from_str::<Value>(line) {
            Ok(request) => request,
            Err(error) => {
                return Some(jsonrpc_error(
                    Value::Null,
                    -32700,
                    &format!("invalid JSON-RPC message: {error}"),
                ));
            }
        };

        let id = request.get("id").cloned();
        let method = request.get("method").and_then(Value::as_str);

        match method {
            Some("initialize") => id.map(|id| {
                let apps = client_supports_apps(&request);
                self.apps_negotiated.store(apps, Ordering::SeqCst);
                jsonrpc_result(
                    id,
                    initialize_result_with_apps(
                        self.backend_name(),
                        self.grpc_tls_mode_name(),
                        apps,
                        self.bridges_languages(),
                    ),
                )
            }),
            Some("notifications/initialized") => None,
            Some("tools/list") => id.map(|id| {
                jsonrpc_result(
                    id,
                    tools_list_result_with_apps(self.apps_negotiated.load(Ordering::SeqCst)),
                )
            }),
            Some("resources/list") if self.apps_negotiated.load(Ordering::SeqCst) => {
                id.map(|id| jsonrpc_result(id, resources_list_result()))
            }
            Some("resources/read") if self.apps_negotiated.load(Ordering::SeqCst) => id.map(|id| {
                let uri = request
                    .get("params")
                    .and_then(|params| params.get("uri"))
                    .and_then(Value::as_str);
                match uri {
                    Some(uri) => match resource_read_result(uri) {
                        Ok(result) => jsonrpc_result(id, result),
                        Err(error) => jsonrpc_error(id, -32002, &error.message),
                    },
                    None => jsonrpc_error(id, -32602, "resources/read requires params.uri"),
                }
            }),
            Some("tools/call") => match id {
                Some(id) => Some(self.handle_tool_call(id, request.get("params")).await),
                None => None,
            },
            Some(other) => id.map(|id| {
                jsonrpc_error(
                    id,
                    -32601,
                    &format!("unsupported JSON-RPC method `{other}`"),
                )
            }),
            None => Some(jsonrpc_error(
                Value::Null,
                -32600,
                "missing JSON-RPC method",
            )),
        }
    }

    /// Handles one newline-delimited JSON-RPC message without trusting the
    /// host to supply UTF-8. A broken line receives the standard parse error;
    /// it does not terminate the stdio session or discard later requests.
    pub async fn handle_json_bytes(&self, line: &[u8]) -> Option<String> {
        match std::str::from_utf8(line) {
            Ok(line) => self.handle_json_line(line).await,
            Err(error) => Some(jsonrpc_error(
                Value::Null,
                -32700,
                &format!("invalid JSON-RPC message: input is not valid UTF-8: {error}"),
            )),
        }
    }

    async fn handle_tool_call(&self, id: Value, params: Option<&Value>) -> String {
        let Some(params) = params.and_then(Value::as_object) else {
            return jsonrpc_error(id, -32602, "tools/call requires object params");
        };
        let Some(requested_name) = params.get("name").and_then(Value::as_str) else {
            return jsonrpc_error(id, -32602, "tools/call requires params.name");
        };
        let name = canonical_tool_name(requested_name);
        let arguments = params.get("arguments").unwrap_or(&Value::Null);
        let start = Instant::now();

        if matches!(name, "kmp_view_read_projection" | "kmp_view_undo")
            && !self.apps_negotiated.load(Ordering::SeqCst)
        {
            return jsonrpc_result(
                id,
                tool_error_result(&ToolError::unknown_tool(format!(
                    "{name} is callable only by a negotiated MCP App"
                ))),
            );
        }

        // Before anything reads them: the schemas declare
        // `additionalProperties: false`, so an argument the tool does not have
        // is refused here rather than dropped and answered anyway.
        if let Err(error) = reject_unknown_arguments(name, arguments) {
            record_tool_error(
                self.backend_name(),
                self.grpc_tls_mode_name(),
                name,
                arguments,
                ToolErrorKind::Validation,
                &error.message,
                start.elapsed(),
            );
            return jsonrpc_result(id, tool_error_result(&error));
        }

        if name == "kmp_write_memory" {
            return self.handle_kmp_write_memory(id, arguments, start).await;
        }

        // The view tools never reach the backend's write path — they hold a
        // view registry and a read-only existence check, and nothing else.
        if crate::serving::view_tools::is_view_tool(name) {
            return self.handle_view_tool(id, name, arguments, start).await;
        }

        match self.backend.call_tool(name, arguments).await {
            Ok(result) => {
                record_tool_success(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    name,
                    arguments,
                    &result,
                    start.elapsed(),
                );
                jsonrpc_result(id, result)
            }
            Err(error) => {
                record_tool_error(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    name,
                    arguments,
                    ToolErrorKind::Backend,
                    &error.message,
                    start.elapsed(),
                );
                jsonrpc_result(id, tool_error_result(&error))
            }
        }
    }
}
