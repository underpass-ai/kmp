//! The commit half every writer shares: a compiled plan goes to canonical
//! ingest with its receipt context and neighbourhood review, and comes back
//! as a pending review, a preview or a commit. One path, so a curated link
//! is reviewed, replayed and receipted exactly as a written one.

use serde_json::{Map, Value};

use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::serving::tool_error::ToolError;
use crate::write::plan::KernelWritePlan;
use crate::write::{write_commit_result, write_dry_run_result};

impl KernelMcpServer {
    pub(super) async fn commit_write_plan(
        &self,
        arguments: &Value,
        plan: &KernelWritePlan,
        evidence_metadata: Option<&Map<String, Value>>,
    ) -> Result<Value, ToolError> {
        let mut ingest_arguments = plan.ingest_arguments.clone();
        ingest_arguments["receipt_context"] = crate::write::receipt::receipt_context(plan);
        // A declared link is reviewed before it commits, whichever shape
        // declared it. Only a search rendering writes no relation at all.
        if plan.operation != crate::write::operation::WriteOperation::SearchSummaries {
            ingest_arguments["neighborhood_review"] = serde_json::json!(
                arguments
                    .get("review_token")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            );
        }
        if let Some(metadata) = evidence_metadata {
            with_evidence_metadata(&mut ingest_arguments, metadata);
        }
        let result = self
            .backend
            .call_tool("kmp_ingest", &ingest_arguments)
            .await?;
        let ingest_result = result.get("structuredContent").cloned().unwrap_or(result);
        Ok(
            if let Some(neighborhood) = ingest_result
                .get("neighborhood")
                .filter(|value| value.is_object())
            {
                super::write_review_result::pending_review(arguments, plan, neighborhood.clone())
            } else if plan.dry_run {
                write_dry_run_result(plan, ingest_result, self.backend_name())
            } else {
                write_commit_result(
                    plan,
                    ingest_result,
                    self.viewer_invitation(),
                    self.orphaned_bundle_notice(),
                )
            },
        )
    }
}

/// Adds the same keys to the metadata of every evidence item the packet
/// writes, keeping whatever metadata an item already carries.
fn with_evidence_metadata(ingest: &mut Value, metadata: &Map<String, Value>) {
    for evidence in ingest
        .pointer_mut("/memory/evidence")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let slot = evidence.as_object_mut().map(|object| {
            object
                .entry("metadata")
                .or_insert_with(|| Value::Object(Map::new()))
        });
        if let Some(Value::Object(existing)) = slot {
            for (key, value) in metadata {
                existing.insert(key.clone(), value.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_evidence_item_gains_the_keys_and_keeps_its_own() {
        let mut ingest = json!({"memory": {"evidence": [
            {"id": "e1", "text": "a"},
            {"id": "e2", "text": "b", "metadata": {"lang": "en"}}
        ]}});
        let metadata = json!({"curated_by": "kmp_curate"})
            .as_object()
            .cloned()
            .expect("map");
        with_evidence_metadata(&mut ingest, &metadata);
        assert_eq!(
            ingest["memory"]["evidence"][0]["metadata"]["curated_by"],
            "kmp_curate"
        );
        assert_eq!(
            ingest["memory"]["evidence"][1]["metadata"]["curated_by"],
            "kmp_curate"
        );
        assert_eq!(ingest["memory"]["evidence"][1]["metadata"]["lang"], "en");
    }
}
