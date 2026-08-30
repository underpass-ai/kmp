//! The view tools' dispatch: they never reach the backend's write path —
//! a view is a camera position, not a record — but every ref an intent
//! names is checked against the store through the same read an agent uses.

use std::time::Instant;

use kmp_viewer::ViewRegistry;
use serde_json::Value;

use crate::serving::json_rpc::jsonrpc_result;
use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::serving::telemetry::{ToolErrorKind, record_tool_error, record_tool_success};
use crate::serving::tool_error::ToolError;
use crate::serving::tool_result::{tool_error_result, tool_success_result};

impl KernelMcpServer {
    /// Moves a view, after checking that every ref the intent names is
    /// really in this store. A view that points at memory which is not there
    /// would draw an empty loom that looks like an answer.
    pub(super) async fn handle_view_tool(
        &self,
        id: Value,
        name: &str,
        arguments: &Value,
        start: Instant,
    ) -> String {
        let outcome = match name {
            "kmp_view_get_state" => {
                crate::serving::view_tools::get_state(arguments, self.viewer_url.as_deref())
            }
            "kmp_view_undo" => crate::serving::view_tools::undo(arguments),
            "kmp_view_open" => {
                let about = arguments.get("about").and_then(Value::as_str).unwrap_or("");
                match self.memory_ref_exists(about, about).await {
                    Ok(exists) => crate::serving::view_tools::open(
                        arguments,
                        exists,
                        self.viewer_url.as_deref(),
                    ),
                    Err(error) => Err(error),
                }
            }
            "kmp_view_apply_intent" => {
                let mut missing = Vec::new();
                let mut failure = None;
                let about = crate::serving::view_tools::about_for_intent(arguments);
                for reference in crate::serving::view_tools::refs_named(arguments) {
                    let Some(about) = about.as_deref() else {
                        break;
                    };
                    match self.memory_ref_exists(about, &reference).await {
                        Ok(true) => {}
                        Ok(false) => missing.push(reference),
                        Err(error) => {
                            failure = Some(error);
                            break;
                        }
                    }
                }
                match failure {
                    Some(error) => Err(error),
                    None => match self.unhonored_projection(arguments).await {
                        Ok(unhonored) => {
                            crate::serving::view_tools::apply_intent(arguments, &missing, unhonored)
                        }
                        Err(error) => Err(error),
                    },
                }
            }
            other => Err(ToolError::unknown_tool(format!(
                "unknown view tool `{other}`"
            ))),
        };

        match outcome {
            Ok(result) => {
                let payload = tool_success_result(result);
                record_tool_success(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    name,
                    arguments,
                    &payload,
                    start.elapsed(),
                );
                jsonrpc_result(id, payload)
            }
            Err(error) => {
                record_tool_error(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    name,
                    arguments,
                    ToolErrorKind::Validation,
                    &error.message,
                    start.elapsed(),
                );
                jsonrpc_result(id, tool_error_result(&error))
            }
        }
    }

    /// Whether one ref is in this store, asked through the same read the
    /// agent would use. Never a write.
    async fn memory_ref_exists(&self, about: &str, reference: &str) -> Result<bool, ToolError> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Ok(false);
        }
        match self
            .backend
            .call_tool(
                "kmp_inspect",
                &serde_json::json!({
                    "about": about,
                    "ref": reference,
                    "include": {"incoming": false, "outgoing": false, "details": false}
                }),
            )
            .await
        {
            Ok(_) => Ok(true),
            Err(error)
                if error.code == crate::serving::tool_error_code::ToolErrorCode::NotFound =>
            {
                Ok(false)
            }
            Err(error) => Err(ToolError::new(
                error.code,
                format!(
                    "could not check whether `{reference}` is in this store: {}",
                    error.message
                ),
            )),
        }
    }

    async fn unhonored_projection(
        &self,
        arguments: &Value,
    ) -> Result<crate::serving::view_tools::UnhonoredProjection, ToolError> {
        let requested = crate::serving::view_tools::projection_names(arguments);
        let mut unhonored = crate::serving::view_tools::UnhonoredProjection::default();
        let about = crate::serving::view_tools::about_for_intent(arguments);

        if !requested.dimensions.is_empty() {
            let Some(about) = about else {
                unhonored.dimensions = requested.dimensions;
                return Ok(unhonored);
            };
            let response = self
                .backend
                .call_tool(
                    "kmp_view_read_projection",
                    &serde_json::json!({
                        "about": about,
                        "from": "0001-01-01T00:00:00Z",
                        "to": "9999-12-31T23:59:59Z",
                        "lod": "atlas",
                        "dimensions": {
                            "mode": "only",
                            "include": requested.dimensions,
                            "scope": "current_about"
                        }
                    }),
                )
                .await
                .map_err(|error| {
                    ToolError::new(
                        error.code,
                        format!("could not resolve the view's dimensions: {}", error.message),
                    )
                })?;
            unhonored.dimensions = response
                .pointer("/structuredContent/coverage/missing")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
        }
        for overlay in requested.overlays {
            if !ViewRegistry::shared().overlay_available(&overlay) {
                unhonored.overlays.push(overlay);
            }
        }
        Ok(unhonored)
    }
}
