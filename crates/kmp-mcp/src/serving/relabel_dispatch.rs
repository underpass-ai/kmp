//! The relabel dispatch: plan from the caller's pairs, then commit through
//! the backend's `kmp_relabel`. The kernel validates against the store on
//! a dry run too, so nothing is answered from the arguments alone.

use std::time::Instant;

use serde_json::Value;

use crate::serving::json_rpc::jsonrpc_result;
use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::serving::telemetry::{
    CallOrigin, ToolErrorKind, record_call_error, record_call_success,
};
use crate::serving::tool_error::ToolError;
use crate::serving::tool_result::{tool_error_result, tool_success_result};
use crate::write::{build_relabel_plan, relabel_result};

impl KernelMcpServer {
    pub(super) async fn handle_kmp_relabel(
        &self,
        id: Value,
        arguments: &Value,
        start: Instant,
        origin: &CallOrigin,
    ) -> String {
        let plan = match build_relabel_plan(arguments) {
            Ok(plan) => plan,
            Err(message) => {
                // Everything the planner refuses is about the arguments;
                // the caller can fix all of it, and only the caller can.
                let error = ToolError::invalid_argument(message);
                record_call_error(
                    self.call_source(origin, false),
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kmp_relabel",
                    arguments,
                    ToolErrorKind::Validation,
                    &error.message,
                    &error.feedback,
                    start.elapsed(),
                );
                return jsonrpc_result(id, tool_error_result("kmp_relabel", arguments, &error));
            }
        };

        match self
            .backend
            .call_tool("kmp_relabel", &plan.relabel_arguments)
            .await
        {
            Ok(result) => {
                let kernel_result = result.get("structuredContent").cloned().unwrap_or(result);
                let result = tool_success_result(relabel_result(&plan, kernel_result));
                record_call_success(
                    self.call_source(origin, false),
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kmp_relabel",
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
                    "kmp_relabel",
                    arguments,
                    ToolErrorKind::Backend,
                    &error.message,
                    &error.feedback,
                    start.elapsed(),
                );
                jsonrpc_result(id, tool_error_result("kmp_relabel", arguments, &error))
            }
        }
    }
}
