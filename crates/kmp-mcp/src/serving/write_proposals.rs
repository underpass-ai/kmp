//! After a committed write of memories, the relations they are missing, as
//! Jev proposes them. The agent declares the ones it confirms through
//! `kmp_curate` apply, in its own why and evidence; nothing is written here.

use serde_json::{Value, json};

use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::write::operation::WriteOperation;
use crate::write::plan::KernelWritePlan;

/// The most just-written memories one proposal review reads.
const FOCUS_REFS: usize = 8;
const EXCERPT_CHARS: usize = 120;

impl KernelMcpServer {
    /// Adds `proposed_relations` to a committed memories write when the store
    /// opted in. Silent otherwise, and a failure only warns: the write stands.
    pub(super) async fn propose_write_relations(&self, plan: &KernelWritePlan, result: &mut Value) {
        if plan.operation != WriteOperation::Memories
            || plan.dry_run
            || result["status"] != "committed"
            || plan.generated_refs.is_empty()
        {
            return;
        }
        let focus = plan
            .generated_refs
            .iter()
            .take(FOCUS_REFS)
            .collect::<Vec<_>>();
        let answer = match self
            .backend
            .call_tool(
                "kmp_curate",
                &json!({"mode": "write_proposals", "about": plan.about,
                        "focus": focus, "page": {"entries": 20}}),
            )
            .await
        {
            Ok(answer) => answer.get("structuredContent").cloned().unwrap_or(answer),
            // A kernel without curation, or a store without the opt-in.
            Err(_) => return,
        };
        if answer["enabled"] == false {
            return;
        }
        let items = answer["missing"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| {
                json!({
                    "item_id": item["item_id"],
                    "from": item["from"]["ref"],
                    "to": item["to"]["ref"],
                    "rel": item["suggested_rel"],
                    "confidence": item["jev"]["confidence"],
                    "to_excerpt": excerpt(item["to"]["excerpt"].as_str().unwrap_or_default()),
                })
            })
            .collect::<Vec<_>>();
        if let Some(warnings) = result["warnings"].as_array_mut() {
            for warning in answer["warnings"].as_array().into_iter().flatten() {
                warnings.push(json!(format!(
                    "relation proposals: {}",
                    warning.as_str().unwrap_or_default()
                )));
            }
        }
        if items.is_empty() {
            return;
        }
        result["proposed_relations"] = json!({
            "review_token": answer["review_token"],
            "items": items,
            "jev": answer["jev"],
            "how": "Relations Jev reads as missing from what you just wrote. Declare the ones you confirm with kmp_curate mode:\"apply\", this review_token and each item_id, in your own why and evidence; ignore the rest.",
        });
    }
}

fn excerpt(text: &str) -> String {
    match text.char_indices().nth(EXCERPT_CHARS) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}
