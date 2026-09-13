//! Compile one semantic or search-summary packet, then validate and commit
//! through canonical ingest. Explicit previews perform validation without writes.

use std::time::Instant;

use kmp_domain::SearchSummary;
use serde_json::{Map, Value, json};

use crate::serving::existing_entry_read::read_existing_entry;
use crate::serving::json_rpc::jsonrpc_result;
use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::serving::telemetry::{ToolErrorKind, record_tool_error, record_tool_success};
use crate::serving::tool_error::ToolError;
use crate::serving::tool_result::{tool_error_result, tool_success_result};
use crate::write::existing_entry::ExistingEntry;
use crate::write::validation_error::WriteValidationError;
use crate::write::validation_errors::WriteValidationErrors;
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
            (Some(_), None) => build_batch_plan(arguments).map_err(ToolError::from),
            (None, Some(_)) => self.plan_search_summary_packet(arguments).await,
            _ => Err(WriteValidationError::new(
                "provide exactly one of memories or search_summaries",
            )
            .code("WRITE_OPERATION_REQUIRED")
            .into()),
        };
        let plan = match planned {
            Ok(plan) => plan,
            Err(error) => {
                // Compiler refusals carry field feedback. A failed source read
                // keeps the category reported by the store (#586).
                record_tool_error(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kmp_write_memory",
                    arguments,
                    if error.code == crate::serving::ToolErrorCode::InvalidArgument {
                        ToolErrorKind::Validation
                    } else {
                        ToolErrorKind::Backend
                    },
                    &error.message,
                    start.elapsed(),
                );
                return jsonrpc_result(
                    id,
                    tool_error_result("kmp_write_memory", arguments, &error),
                );
            }
        };

        let mut ingest_arguments = plan.ingest_arguments.clone();
        ingest_arguments["receipt_context"] = crate::write::receipt::receipt_context(&plan);
        if arguments.get("memories").is_some() {
            ingest_arguments["neighborhood_review"] = serde_json::json!(
                arguments
                    .get("review_token")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            );
        }
        match self
            .backend
            .call_tool("kmp_ingest", &ingest_arguments)
            .await
        {
            Ok(result) => {
                let ingest_result = result.get("structuredContent").cloned().unwrap_or(result);
                let result = tool_success_result(
                    if let Some(neighborhood) = ingest_result
                        .get("neighborhood")
                        .filter(|value| value.is_object())
                    {
                        super::write_review_result::pending_review(
                            arguments,
                            &plan,
                            neighborhood.clone(),
                        )
                    } else if plan.dry_run {
                        write_dry_run_result(&plan, ingest_result, self.backend_name())
                    } else {
                        write_commit_result(
                            &plan,
                            ingest_result,
                            self.viewer_invitation(),
                            self.orphaned_bundle_notice(),
                        )
                    },
                );
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
                jsonrpc_result(id, tool_error_result("kmp_write_memory", arguments, &error))
            }
        }
    }

    /// Read every target before committing the packet. A rendering changes only
    /// search metadata; the authoritative text, kind and clocks come from storage.
    async fn plan_search_summary_packet(
        &self,
        arguments: &Value,
    ) -> Result<crate::write::plan::KernelWritePlan, ToolError> {
        use crate::write::validated_arguments::{required_map_string, required_string};
        let object = arguments
            .as_object()
            .ok_or_else(|| ToolError::invalid_argument("tool arguments must be an object"))?;
        let about = required_string(object, "about")?;
        for field in [
            "labels",
            "occurred_at",
            "valid_from",
            "valid_until",
            "rank",
            "read_context",
            "review_token",
        ] {
            if object.contains_key(field) {
                return Err(WriteValidationError::new(format!(
                    "search_summaries preserves stored source and coordinates; `{field}` cannot accompany it"
                )).at(field).code("PRESERVED_FIELD").into());
            }
        }
        for field in ["sequence", "labels_new"] {
            if arguments
                .get("options")
                .is_some_and(|options| options.get(field).is_some())
            {
                return Err(WriteValidationError::new(format!(
                    "options.{field} does not apply to search_summaries"
                ))
                .at(format!("options.{field}"))
                .code("INAPPLICABLE_OPTION")
                .into());
            }
        }
        let records = object
            .get("search_summaries")
            .and_then(Value::as_array)
            .filter(|records| !records.is_empty())
            .ok_or_else(|| {
                WriteValidationError::new("search_summaries must be a non-empty array")
                    .at("search_summaries")
            })?;
        let mut targets = std::collections::BTreeSet::new();
        let mut prepared = Vec::new();
        let mut source_bindings = Vec::new();
        for (index, record) in records.iter().enumerate() {
            let record = record.as_object().ok_or_else(|| {
                WriteValidationError::new(format!("search_summaries[{index}] must be an object"))
                    .at(format!("search_summaries[{index}]"))
            })?;
            let reference =
                required_map_string(record, "ref", &format!("search_summaries[{index}].ref"))?;
            kmp_application::validate_supplied_entry_ref(
                &about,
                &format!("search_summaries[{index}].ref"),
                reference,
            )
            .map_err(|error| {
                WriteValidationError::new(error).at(format!("search_summaries[{index}].ref"))
            })?;
            if !targets.insert(reference) {
                return Err(WriteValidationError::new(format!(
                    "search_summaries[{index}].ref repeats target `{reference}`"
                ))
                .at(format!("search_summaries[{index}].ref"))
                .code("DUPLICATE_TARGET")
                .into());
            }
            let existing = read_existing_entry(self.backend.as_ref(), &about, reference).await?;
            source_bindings.push(summary_source_binding(&existing));
            prepared.push((index, record.clone(), existing));
        }
        let explicit_identity = object
            .get("idempotency_key")
            .and_then(Value::as_str)
            .filter(|key| !key.trim().is_empty())
            .map(str::to_owned);
        let implicit_identity = explicit_identity.is_none().then(|| {
            reusable_summary_packet_identity(object, &prepared)
                .unwrap_or_else(|| derived_summary_packet_identity(object, source_bindings))
        });
        let identity = explicit_identity
            .as_ref()
            .or(implicit_identity.as_ref())
            .expect("one summary identity always exists");
        let mut plans = Vec::new();
        let mut errors = Vec::new();
        for (index, record, existing) in prepared {
            let mut request = object.clone();
            request.remove("search_summaries");
            request.insert("current".into(), Value::Object(record));
            request.insert("idempotency_key".into(), serde_json::json!(identity));
            if let Some(implicit_identity) = &implicit_identity {
                request.insert(
                    "summary_validation_identity".into(),
                    serde_json::json!(implicit_identity),
                );
            }
            let mut plan = match build_summary_plan(&Value::Object(request), &existing) {
                Ok(plan) => plan,
                Err(error) => {
                    errors.push(error.within(&format!("search_summaries[{index}]")));
                    continue;
                }
            };
            for action in &mut plan.next_suggested_reads {
                action["about"] = serde_json::json!(about);
            }
            plans.push(plan);
        }
        if let Some(errors) = WriteValidationErrors::collected(errors) {
            return Err(errors.into());
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

/// A new implicit summary packet binds to the current source revision and
/// text. Its stored identity becomes the retry identity only when all targets
/// still carry the exact actor, summary, and source fingerprint it validated.
fn derived_summary_packet_identity(
    arguments: &Map<String, Value>,
    source_bindings: Vec<Value>,
) -> String {
    let mut source_bound = arguments.clone();
    source_bound.insert(
        "summary_source_bindings".to_string(),
        json!(source_bindings),
    );
    crate::write::generated_ref::stable_idempotency_key(&source_bound)
}

fn reusable_summary_packet_identity(
    arguments: &Map<String, Value>,
    prepared: &[(usize, Map<String, Value>, ExistingEntry)],
) -> Option<String> {
    let actor = arguments.get("actor")?.as_str()?;
    let mut identities = prepared.iter().map(|(_, record, existing)| {
        let summary = record.get("summary_en")?.as_str()?;
        let fingerprint = SearchSummary::source_fingerprint(&existing.text);
        let identity = existing
            .metadata
            .get(SearchSummary::VALIDATION_IDENTITY_METADATA_KEY)?
            .as_str()?;
        (existing
            .metadata
            .get(SearchSummary::METADATA_KEY)?
            .as_str()?
            == summary
            && existing
                .metadata
                .get(SearchSummary::SUMMARY_WRITER_METADATA_KEY)?
                .as_str()?
                == actor
            && existing
                .metadata
                .get(SearchSummary::SOURCE_FINGERPRINT_METADATA_KEY)?
                .as_str()?
                == fingerprint)
            .then_some(identity.to_string())
    });
    let identity = identities.next()??;
    identities
        .all(|candidate| candidate.as_deref() == Some(identity.as_str()))
        .then_some(identity)
}

fn summary_source_binding(existing: &ExistingEntry) -> Value {
    let fingerprint = SearchSummary::source_fingerprint(&existing.text);
    json!({
        "ref": existing.reference.as_str(),
        "source_revision": existing.revision,
        "source_sha256": fingerprint
    })
}
