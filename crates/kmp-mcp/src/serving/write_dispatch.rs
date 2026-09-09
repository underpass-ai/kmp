//! Compile one semantic or search-summary packet, then validate and commit
//! through canonical ingest. Explicit previews perform validation without writes.

use std::time::Instant;

use serde_json::Value;

use crate::serving::existing_entry_read::read_existing_entry;
use crate::serving::json_rpc::jsonrpc_result;
use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::serving::telemetry::{ToolErrorKind, record_tool_error, record_tool_success};
use crate::serving::tool_error::ToolError;
use crate::serving::tool_result::{tool_error_result, tool_success_result};
use crate::write::{
    build_batch_plan, build_summary_plan, write_commit_result, write_dry_run_result,
};

impl KernelMcpServer {
    pub(super) async fn handle_kmp_write_memory(
        &self,
        id: Value,
        arguments: &Value,
        start: Instant,
    ) -> String {
        let planned = match (arguments.get("memories"), arguments.get("search_summaries")) {
            (Some(_), None) => build_batch_plan(arguments),
            (None, Some(_)) => self.plan_search_summary_packet(arguments).await,
            _ => Err("provide exactly one of memories or search_summaries".to_string()),
        };
        let plan = match planned {
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

        match self
            .backend
            .call_tool("kmp_ingest", &plan.ingest_arguments)
            .await
        {
            Ok(result) => {
                let ingest_result = result.get("structuredContent").cloned().unwrap_or(result);
                let result = tool_success_result(if plan.dry_run {
                    write_dry_run_result(&plan, ingest_result, self.backend_name())
                } else {
                    write_commit_result(
                        &plan,
                        ingest_result,
                        self.viewer_invitation(),
                        self.orphaned_bundle_notice(),
                    )
                });
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

    /// Read every target before committing the packet. A rendering changes only
    /// search metadata; the authoritative text, kind and clocks come from storage.
    async fn plan_search_summary_packet(
        &self,
        arguments: &Value,
    ) -> Result<crate::write::plan::KernelWritePlan, String> {
        use crate::write::arguments::{required_map_string, required_string};
        let object = arguments
            .as_object()
            .ok_or("tool arguments must be an object")?;
        let about = required_string(object, "about")?;
        for field in [
            "labels",
            "occurred_at",
            "valid_from",
            "valid_until",
            "rank",
            "read_context",
        ] {
            if object.contains_key(field) {
                return Err(format!(
                    "search_summaries preserves stored source and coordinates; `{field}` cannot accompany it"
                ));
            }
        }
        for field in ["sequence", "labels_new"] {
            if arguments
                .get("options")
                .is_some_and(|options| options.get(field).is_some())
            {
                return Err(format!(
                    "options.{field} does not apply to search_summaries"
                ));
            }
        }
        let records = object
            .get("search_summaries")
            .and_then(Value::as_array)
            .filter(|records| !records.is_empty())
            .ok_or("search_summaries must be a non-empty array")?;
        let identity = object
            .get("idempotency_key")
            .and_then(Value::as_str)
            .filter(|key| !key.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| crate::write::generated_ref::stable_idempotency_key(object));
        let mut targets = std::collections::BTreeSet::new();
        let mut plans = Vec::new();
        for (index, record) in records.iter().enumerate() {
            let record = record
                .as_object()
                .ok_or_else(|| format!("search_summaries[{index}] must be an object"))?;
            let reference =
                required_map_string(record, "ref", &format!("search_summaries[{index}].ref"))?;
            kmp_application::validate_supplied_entry_ref(
                &about,
                &format!("search_summaries[{index}].ref"),
                reference,
            )?;
            if !targets.insert(reference) {
                return Err(format!(
                    "search_summaries[{index}].ref repeats target `{reference}`"
                ));
            }
            let existing = read_existing_entry(self.backend.as_ref(), &about, reference).await?;
            let mut request = object.clone();
            request.remove("search_summaries");
            request.insert("current".into(), Value::Object(record.clone()));
            request.insert("idempotency_key".into(), serde_json::json!(identity));
            let mut plan = build_summary_plan(&Value::Object(request), &existing)
                .map_err(|error| format!("search_summaries[{index}]: {error}"))?;
            for action in &mut plan.next_suggested_reads {
                action["about"] = serde_json::json!(about);
            }
            plans.push(plan);
        }
        let mut result = plans.remove(0);
        for plan in plans {
            let entries = plan.ingest_arguments["memory"]["entries"]
                .as_array()
                .expect("compiled entries");
            result.ingest_arguments["memory"]["entries"]
                .as_array_mut()
                .expect("compiled entries")
                .extend(entries.iter().cloned());
            result.diagnostics.extend(plan.diagnostics);
            result
                .next_suggested_reads
                .extend(plan.next_suggested_reads);
        }
        Ok(result)
    }
}
