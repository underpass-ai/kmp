use serde_json::{Value, json};

use super::plan::KernelWritePlan;

/// The compiler's interpretation accompanies the canonical write in its audit
/// event. Source text, actual assigned clocks and sequence come from the kernel.
pub(crate) fn receipt_context(plan: &KernelWritePlan) -> Value {
    json!({
        "coverage": super::coverage::write_coverage(plan),
        "local_refs": plan.local_refs,
        "generated_refs": plan.generated_refs,
        "labels": plan.labels,
        "relations": plan.relations,
        "relation_quality": plan.relation_quality,
        "relation_quality_metrics": plan.relation_quality_metrics,
        "diagnostics": plan.diagnostics
    })
}

pub(super) fn receipt_action(about: &str, reference: &str) -> Value {
    json!({
        "tool": "kmp_inspect",
        "arguments": {
            "about": about,
            "ref": reference,
            "include": {"details": true, "incoming": false, "outgoing": false, "raw": false}
        }
    })
}

/// Describe planned defaults separately from actual accepted clock values.
/// Absence means this packet needed no implicit observation.
pub(super) fn observation_defaults(plan: &KernelWritePlan) -> Option<Value> {
    let entries = plan.ingest_arguments["memory"]["entries"].as_array()?;
    let defaulted = entries
        .iter()
        .filter(|entry| {
            entry["coordinates"].as_array().is_some_and(|coordinates| {
                coordinates
                    .iter()
                    .any(|c| c["observed_at"].is_null() && c["ingested_at"].is_null())
            })
        })
        .count();
    let provenance = plan.ingest_arguments["provenance"]["observed_at"].is_null();
    (defaulted > 0 || provenance).then(|| {
        json!({
            "observed_at": "ingested_at", "entries": defaulted, "provenance": provenance
        })
    })
}
