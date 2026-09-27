//! Judged search expansions at write (P15, Doc2Query--).
//!
//! A memories write may carry, per memory, `search_expansions`: short ways
//! a later reader may ask for it. They are never part of the memory the
//! write commits. Once the memories stand, the expansions are read by the
//! lint and by Jev against the stored text, and those Jev accepts at the
//! store's bar are attached as a second, metadata-only write — the same path
//! `search_summaries` takes. Without the store's opt-in or a working Jev,
//! nothing is attached and the result says why.

use serde_json::{Value, json};

use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::write::expansion_selection::ExpansionSelection;
use crate::write::operation::WriteOperation;
use crate::write::plan::KernelWritePlan;

impl KernelMcpServer {
    /// Asks the backend's judge about what passed the lint and applies its
    /// answer. A backend without the judge keeps nothing.
    pub(super) async fn judge_expansion_selection(
        &self,
        about: &str,
        selection: &mut ExpansionSelection,
    ) {
        if !selection.needs_judgement() {
            return;
        }
        match self
            .backend
            .call_tool("kmp_curate", &selection.judge_arguments(about))
            .await
        {
            Ok(answer) => {
                selection.apply(answer.get("structuredContent").unwrap_or(&answer));
            }
            Err(error) => selection.not_judged(format!(
                "this backend cannot judge search expansions: {}",
                error.message
            )),
        }
    }

    /// After a committed memories write, attaches the expansions its
    /// memories proposed, and reports them as `search_expansions`.
    pub(super) async fn attach_written_expansions(
        &self,
        arguments: &Value,
        plan: &KernelWritePlan,
        result: &mut Value,
    ) {
        if plan.operation != WriteOperation::Memories {
            return;
        }
        let proposed = arguments
            .get("memories")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|memory| {
                let expansions = memory.get("search_expansions")?.as_array()?;
                let reference = plan.local_refs.get(memory.get("id")?.as_str()?)?;
                (!expansions.is_empty())
                    .then(|| json!({"ref": reference, "search_expansions": expansions}))
            })
            .collect::<Vec<_>>();
        if proposed.is_empty() {
            return;
        }
        if plan.dry_run {
            result["search_expansions"] = json!({"stored": {}, "refused": [], "not_stored": "a preview judges and stores no expansion"});
            return;
        }
        if !matches!(result["status"].as_str(), Some("committed" | "replayed")) {
            return;
        }
        let mut attach = json!({
            "about": plan.about,
            "actor": arguments.get("actor").cloned().unwrap_or(Value::Null),
            "search_summaries": proposed,
        });
        if let Some(source_kind) = arguments.get("source_kind") {
            attach["source_kind"] = source_kind.clone();
        }
        let report = match self.plan_search_summary_packet(&attach).await {
            Ok((attach_plan, report)) => {
                match self.commit_write_plan(&attach, &attach_plan, None).await {
                    Ok(_) => report.unwrap_or_default(),
                    Err(error) => json!({"stored": {}, "refused": [],
                        "not_stored": format!("the expansions could not be attached: {}", error.message)}),
                }
            }
            Err(error) => json!({"stored": {}, "refused": [],
                "not_stored": error.message}),
        };
        result["search_expansions"] = report;
    }
}
