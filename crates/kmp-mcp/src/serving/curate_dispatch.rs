//! `kmp_curate` apply: the store that froze the review resolves and
//! pre-checks what the agent accepted, then the links go through the
//! writer's own plan, neighbourhood review, idempotency and receipt. A
//! resumed review comes back here, so provenance and frozen doubts survive.

use std::time::Instant;

use serde_json::{Map, Value, json};

use crate::serving::json_rpc::jsonrpc_result;
use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::serving::telemetry::{ToolErrorKind, record_tool_error, record_tool_success};
use crate::serving::tool_error::ToolError;
use crate::serving::tool_result::{tool_error_result, tool_success_result};
use crate::write::build_relation_plan;

const TOOL: &str = "kmp_curate";

impl KernelMcpServer {
    pub(super) async fn handle_kmp_curate_apply(
        &self,
        id: Value,
        arguments: &Value,
        start: Instant,
    ) -> String {
        match self.curate_apply(arguments).await {
            Ok(value) => {
                let result = tool_success_result(value);
                record_tool_success(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    TOOL,
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
                    TOOL,
                    arguments,
                    if error.code == crate::serving::ToolErrorCode::InvalidArgument {
                        ToolErrorKind::Validation
                    } else {
                        ToolErrorKind::Backend
                    },
                    &error.message,
                    start.elapsed(),
                );
                jsonrpc_result(id, tool_error_result(TOOL, arguments, &error))
            }
        }
    }

    async fn curate_apply(&self, arguments: &Value) -> Result<Value, ToolError> {
        let about = arguments
            .get("about")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::invalid_argument("kmp_curate apply requires about"))?;
        let actor = arguments
            .get("actor")
            .and_then(Value::as_str)
            .filter(|actor| !actor.trim().is_empty())
            .ok_or_else(|| {
                ToolError::invalid_argument(
                    "kmp_curate apply writes: name `actor`, or pass the context_id kmp_guide returned",
                )
            })?;
        let prepared = self
            .backend
            .call_tool(
                TOOL,
                &json!({
                    "mode": "prepare_apply",
                    "about": about,
                    "review_token": arguments.get("review_token").cloned().unwrap_or(Value::Null),
                    "accepted": arguments.get("accepted").cloned().unwrap_or(Value::Null),
                }),
            )
            .await?;
        let prepared = prepared
            .get("structuredContent")
            .cloned()
            .unwrap_or(prepared);
        let relations = prepared["relations"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let curate = json!({
            "doubted": prepared["doubted"],
            "rejected": prepared["rejected"],
            "jev": prepared["jev"],
            "warnings": prepared["warnings"],
        });
        if relations.is_empty() {
            return Ok(json!({
                "status": "nothing_written",
                "summary": "Nothing written: every accepted item was doubted by Jev or cannot be written by this call. Reread the doubted ones and confirm or correct them; apply rejected ones where their reason says.",
                "curate": curate,
                "next_actions": [],
            }));
        }

        let mut write = json!({
            "about": about,
            "actor": actor,
            "relations": relations.iter().map(|relation| {
                let mut link = json!({
                    "from": relation["from"], "to": relation["to"], "rel": relation["rel"],
                    "why": relation["why"], "evidence": relation["evidence"],
                });
                if relation["confidence"].is_string() {
                    link["confidence"] = relation["confidence"].clone();
                }
                link
            }).collect::<Vec<_>>(),
        });
        let proposals = relations
            .iter()
            .filter(|relation| relation["proposal"].is_array())
            .map(|relation| {
                json!({"from": relation["from"], "to": relation["to"], "proposed_by": relation["proposal"]})
            })
            .collect::<Vec<_>>();
        if !proposals.is_empty() {
            write["read_context"] = json!({"relate_proposals": proposals});
        }
        for (from, to) in [
            ("write_review_token", "review_token"),
            ("idempotency_key", "idempotency_key"),
        ] {
            if let Some(value) = arguments.get(from).filter(|value| value.is_string()) {
                write[to] = value.clone();
            }
        }
        let plan = build_relation_plan(&write).map_err(ToolError::from)?;

        let mut metadata = Map::new();
        metadata.insert("curated_by".into(), json!(TOOL));
        // The model that checked these items, frozen with its doubts, so a
        // resumed apply writes the same packet and the same provenance.
        if let Some(model) = prepared["checked_by"].as_str() {
            metadata.insert("curated_with".into(), json!(model));
        }
        if relations
            .iter()
            .any(|relation| relation["proposed_by"] == "jev")
        {
            metadata.insert("proposed_by".into(), json!("jev"));
        }
        let mut result = self
            .commit_write_plan(&write, &plan, Some(&metadata))
            .await?;
        if result["status"] == "needs_review" {
            // Resume here, not in kmp_write_memory: the kernel's review token
            // covers this packet's provenance, and the doubts stay frozen.
            let mut resume = arguments.clone();
            resume["write_review_token"] =
                result["next_actions"][0]["arguments"]["review_token"].clone();
            resume["idempotency_key"] = plan.ingest_arguments["idempotency_key"].clone();
            result["next_actions"] = json!([{"tool": TOOL, "arguments": resume}]);
        }
        result["curate"] = curate;
        Ok(result)
    }
}
