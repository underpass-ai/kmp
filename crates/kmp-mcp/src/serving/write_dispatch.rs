//! The writer's dispatch: plan, preflight the one unlinked-root allowance,
//! then commit through canonical ingest. A dry run answers without writing.

use std::time::Instant;

use serde_json::Value;

use crate::serving::json_rpc::jsonrpc_result;
use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::serving::telemetry::{ToolErrorKind, record_tool_error, record_tool_success};
use crate::serving::tool_error::ToolError;
use crate::serving::tool_result::{tool_error_result, tool_success_result};
use crate::write::{build_write_plan_with_root, write_commit_result, write_dry_run_result};

impl KernelMcpServer {
    pub(super) async fn handle_kmp_write_memory(
        &self,
        id: Value,
        arguments: &Value,
        start: Instant,
    ) -> String {
        let allow_unlinked_root = match self.allow_unlinked_strict_root(arguments).await {
            Ok(allowed) => allowed,
            Err(error) => {
                record_tool_error(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kmp_write_memory",
                    arguments,
                    ToolErrorKind::Backend,
                    &error.message,
                    start.elapsed(),
                );
                return jsonrpc_result(id, tool_error_result(&error));
            }
        };
        let plan = match build_write_plan_with_root(arguments, allow_unlinked_root) {
            Ok(plan) => plan,
            Err(message) => {
                // Everything the write planner refuses is about the
                // arguments: a missing field, an unsupported relation, a rich
                // link with no evidence. The caller can fix all of it, and
                // only the caller can.
                let error = ToolError::invalid_argument(message);
                record_tool_error(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kmp_write_memory",
                    arguments,
                    ToolErrorKind::Validation,
                    &error.message,
                    start.elapsed(),
                );
                return jsonrpc_result(id, tool_error_result(&error));
            }
        };

        if plan.dry_run {
            let result = tool_success_result(write_dry_run_result(&plan));
            record_tool_success(
                self.backend_name(),
                self.grpc_tls_mode_name(),
                "kmp_write_memory",
                arguments,
                &result,
                start.elapsed(),
            );
            return jsonrpc_result(id, result);
        }

        match self
            .backend
            .call_tool("kmp_ingest", &plan.ingest_arguments)
            .await
        {
            Ok(result) => {
                let ingest_result = result.get("structuredContent").cloned().unwrap_or(result);
                let result = tool_success_result(write_commit_result(
                    &plan,
                    ingest_result,
                    self.viewer_invitation(),
                    self.orphaned_bundle_notice(),
                ));
                record_tool_success(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kmp_write_memory",
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
                    "kmp_write_memory",
                    arguments,
                    ToolErrorKind::Backend,
                    &error.message,
                    start.elapsed(),
                );
                jsonrpc_result(id, tool_error_result(&error))
            }
        }
    }

    async fn allow_unlinked_strict_root(&self, arguments: &Value) -> Result<bool, ToolError> {
        let Some(object) = arguments.as_object() else {
            return Ok(false);
        };
        let strict = object
            .get("options")
            .and_then(Value::as_object)
            .and_then(|options| options.get("strict"))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let has_links = object
            .get("connect_to")
            .and_then(Value::as_array)
            .is_some_and(|links| !links.is_empty());
        if !strict || has_links {
            return Ok(false);
        }
        let Some(about) = object
            .get("about")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|about| !about.is_empty())
        else {
            return Ok(false);
        };

        match self
            .backend
            .call_tool(
                "kmp_inspect",
                &serde_json::json!({
                    "about": about,
                    "ref": about,
                    "include": {"incoming": false, "outgoing": false, "details": false}
                }),
            )
            .await
        {
            Ok(_) => Ok(false),
            // The kernel says which failure this was, so this no longer has
            // to guess from the words — and "not found" here is the whole
            // point: an about nobody has written yet is allowed one unlinked
            // root entry.
            Err(error)
                if error.code == crate::serving::tool_error_code::ToolErrorCode::NotFound =>
            {
                Ok(true)
            }
            Err(error) => Err(ToolError::new(
                error.code,
                format!(
                    "kmp_write_memory could not verify whether `{about}` is a new about: {}",
                    error.message
                ),
            )),
        }
    }
}
