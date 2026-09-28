//! JSON-RPC method routing: one newline-delimited message in, at most one
//! response out. Tool calls validate before anything reads the arguments.

use std::time::Instant;

use serde_json::Value;

use super::lexical_order::{LexicalOrder, is_tool_error};
use crate::contract::{
    canonical_tool_name, initialize_result_with_apps, reject_invalid_arguments,
    reject_unknown_arguments, resource_read_result, resources_list_result,
    tools_list_result_with_apps, without_server_checked_constraints,
};
use crate::serving::json_rpc::{jsonrpc_error, jsonrpc_result};
use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::serving::mcp_session::McpSession;
use crate::serving::telemetry::{
    CallOrigin, ToolErrorKind, record_call_error, record_call_success,
};
use crate::serving::tool_error::ToolError;
use crate::serving::tool_result::tool_error_result;

impl KernelMcpServer {
    /// One message of this process's own session (stdio).
    pub async fn handle_json_line(&self, line: &str) -> Option<String> {
        self.handle_json_line_in(line, &self.session).await
    }

    /// One message of `session`: what it negotiated decides the tool
    /// surface and names the host in the call log. The HTTP gateway passes
    /// the session of the request it serves.
    pub async fn handle_json_line_in(&self, line: &str, session: &McpSession) -> Option<String> {
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
                session.initialize(&request);
                let apps = session.apps();
                jsonrpc_result(
                    id,
                    self.passage_initialize(initialize_result_with_apps(
                        self.backend_name(),
                        self.grpc_tls_mode_name(),
                        apps,
                        self.bridges_languages(),
                    )),
                )
            }),
            Some("notifications/initialized") => None,
            Some("tools/list") => id.map(|id| {
                jsonrpc_result(
                    id,
                    self.output_schema_tools(self.passage_tools(
                        without_server_checked_constraints(tools_list_result_with_apps(
                            session.apps(),
                        )),
                    )),
                )
            }),
            Some("resources/list") if session.apps() => {
                id.map(|id| jsonrpc_result(id, resources_list_result()))
            }
            Some("resources/read") if session.apps() => id.map(|id| {
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
                Some(id) => Some(
                    self.handle_tool_call(id, request.get("params"), session)
                        .await,
                ),
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

    async fn handle_tool_call(
        &self,
        id: Value,
        params: Option<&Value>,
        session: &McpSession,
    ) -> String {
        let Some(params) = params.and_then(Value::as_object) else {
            return jsonrpc_error(id, -32602, "tools/call requires object params");
        };
        let Some(requested_name) = params.get("name").and_then(Value::as_str) else {
            return jsonrpc_error(id, -32602, "tools/call requires params.name");
        };
        let name = canonical_tool_name(requested_name);
        let arguments = params.get("arguments").unwrap_or(&Value::Null);
        let start = Instant::now();
        let origin = CallOrigin::read(arguments, session.client());

        if matches!(
            name,
            "kmp_view_read_projection"
                | "kmp_view_read_nodes"
                | "kmp_view_undo"
                | "kmp_view_take_control"
        ) && !session.apps()
        {
            return jsonrpc_result(
                id,
                tool_error_result(
                    name,
                    arguments,
                    &ToolError::unknown_tool(format!(
                        "{name} is callable only by a negotiated MCP App"
                    )),
                ),
            );
        }

        // The four per-move tools became one. Their names answer with the
        // call that replaces them rather than a bare unknown tool.
        if let Some(movement) = crate::contract::TimeMove::retired(name) {
            return jsonrpc_result(
                id,
                tool_error_result(
                    name,
                    arguments,
                    &ToolError::unknown_tool(format!(
                        "{name} was replaced by kmp_time: call kmp_time with \"move\": \"{}\" and \
                         the cursor as `{}`; the other arguments are unchanged.",
                        movement.as_str(),
                        movement.cursor_key()
                    )),
                ),
            );
        }

        // Before anything reads them: the schemas declare
        // `additionalProperties: false`, so an argument the tool does not have
        // is refused here rather than dropped and answered anyway. Empty and
        // repeated values are refused too (#850), but a tool that names the
        // problem more precisely goes first: the writer's planner and the
        // read projections run their own checks, and this one follows them as
        // a backstop. Every other write is checked here, before it commits.
        let lexical_first = LexicalOrder::of(name) == LexicalOrder::BeforeDispatch;
        let checked = if lexical_first {
            reject_invalid_arguments(name, arguments)
        } else {
            reject_unknown_arguments(name, arguments)
        };
        if let Err(error) = checked {
            record_call_error(
                self.call_source(&origin, false),
                self.backend_name(),
                self.grpc_tls_mode_name(),
                name,
                arguments,
                ToolErrorKind::Validation,
                &error.message,
                &error.feedback,
                start.elapsed(),
            );
            return jsonrpc_result(id, tool_error_result(name, arguments, &error));
        }

        if name == "kmp_guide" {
            return self.handle_kmp_guide(id, arguments, start, &origin).await;
        }

        let resolved = match self.resolve_read_arguments(name, arguments) {
            Ok(resolved) => resolved,
            Err(error) => return jsonrpc_result(id, tool_error_result(name, arguments, &error)),
        };
        let arguments = resolved.as_ref().unwrap_or(arguments);
        let guidance = match super::call_guidance::CallGuidance::prepare(self, arguments) {
            Ok(guidance) => guidance,
            Err(error) => return jsonrpc_result(id, tool_error_result(name, arguments, &error)),
        };
        let memory_arguments = guidance
            .as_ref()
            .map(|g| g.memory_arguments(name, arguments));
        let reply_id = id.clone();
        let result = self
            .dispatch_memory_call(
                id,
                name,
                memory_arguments.as_ref().unwrap_or(arguments),
                start,
                &origin,
            )
            .await;
        let result = match guidance {
            Some(guidance) => {
                let result = self.shorten_read_actions(Some(&guidance), result);
                self.complete_work_guidance(name, arguments, &guidance, result)
            }
            None => self.shorten_read_actions(None, result),
        };
        if LexicalOrder::of(name) == LexicalOrder::AfterRead
            && !is_tool_error(&result)
            && let Err(error) = reject_invalid_arguments(name, arguments)
        {
            record_call_error(
                self.call_source(&origin, false),
                self.backend_name(),
                self.grpc_tls_mode_name(),
                name,
                arguments,
                ToolErrorKind::Validation,
                &error.message,
                &error.feedback,
                start.elapsed(),
            );
            return jsonrpc_result(reply_id, tool_error_result(name, arguments, &error));
        }
        self.project_read_passages(name, result)
    }

    async fn dispatch_memory_call(
        &self,
        id: Value,
        name: &str,
        arguments: &Value,
        start: Instant,
        origin: &CallOrigin,
    ) -> String {
        if name == "kmp_write_memory" {
            return self
                .handle_kmp_write_memory(id, arguments, start, origin)
                .await;
        }

        // A relabel is a write the kernel validates against the store, so it
        // is planned from the caller's pairs and committed through the
        // backend's own relabel, never through ingest.
        if name == "kmp_relabel" {
            return self.handle_kmp_relabel(id, arguments, start, origin).await;
        }

        // Curation reviews on the backend; applying writes, so it goes
        // through the writer's own commit path from here.
        if name == "kmp_curate" && arguments.get("mode").and_then(Value::as_str) == Some("apply") {
            return self
                .handle_kmp_curate_apply(id, arguments, start, origin)
                .await;
        }

        // Raw ingest keeps the about boundary whole. The kernel admits one
        // relation across abouts — an equivalence a writer declared from a
        // kmp_relate proposal — but only `kmp_write_memory` may declare it:
        // a raw call is validated against the boundary here, before the
        // backend, and the writer's compiled ingest never passes this way.
        if name == "kmp_ingest"
            && let Err(message) = crate::write::reject_refs_outside_about(arguments)
        {
            let error = ToolError::invalid_argument(message);
            record_call_error(
                self.call_source(origin, false),
                self.backend_name(),
                self.grpc_tls_mode_name(),
                name,
                arguments,
                ToolErrorKind::Validation,
                &error.message,
                &error.feedback,
                start.elapsed(),
            );
            return jsonrpc_result(id, tool_error_result(name, arguments, &error));
        }

        // The view tools never reach the backend's write path — they hold a
        // view registry and a read-only existence check, and nothing else.
        if crate::serving::view_tools::is_view_tool(name) {
            return self
                .handle_view_tool(id, name, arguments, start, origin)
                .await;
        }

        let caller = origin
            .client
            .as_ref()
            .map(|client| (client.name.as_str(), client.version.as_str()));
        match self.backend.call_tool_for(name, arguments, caller).await {
            Ok(result) => {
                // A wake or an ask that answered is the first moment the
                // store surely exists: its salt may be created then.
                let recall = matches!(name, "kmp_ask" | "kmp_wake");
                record_call_success(
                    self.call_source(origin, recall),
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
                record_call_error(
                    self.call_source(origin, false),
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    name,
                    arguments,
                    ToolErrorKind::Backend,
                    &error.message,
                    &error.feedback,
                    start.elapsed(),
                );
                jsonrpc_result(id, tool_error_result(name, arguments, &error))
            }
        }
    }
}
