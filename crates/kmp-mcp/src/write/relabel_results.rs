use serde_json::{Value, json};

use super::relabel_plan::RelabelPlan;

/// The caller's view of a relabel: what the kernel did beside what was
/// asked, with the labels the memory stands in now.
pub(crate) fn relabel_result(plan: &RelabelPlan, kernel_result: Value) -> Value {
    let memory = kernel_result.get("memory").cloned().unwrap_or(Value::Null);
    let committed = memory["read_after_write_ready"].as_bool().unwrap_or(false);
    let created_dimensions = memory["created_dimensions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let created = plan
        .add
        .iter()
        .filter(|label| {
            let value = label["value"].as_str().unwrap_or_default();
            let namespaced = format!("about:{}:dimension:{value}", plan.about);
            created_dimensions
                .iter()
                .any(|dimension| dimension.as_str() == Some(namespaced.as_str()))
        })
        .cloned()
        .collect::<Vec<_>>();
    json!({
        "accepted": committed && !plan.dry_run,
        "dry_run": plan.dry_run,
        "summary": kernel_result["summary"].as_str().unwrap_or_default(),
        "ref": plan.reference,
        "labels": {
            "added": memory["added"].as_array().cloned().unwrap_or_default(),
            "removed": memory["removed"].as_array().cloned().unwrap_or_default(),
            "now": memory["labels"].as_array().cloned().unwrap_or_default(),
            "created": created,
            "resembling": memory["resembling_labels"].as_array().cloned().unwrap_or_default()
        },
        "warnings": kernel_result["warnings"].as_array().cloned().unwrap_or_default(),
        "next_suggested_reads": [{
            "tool": "kmp_inspect",
            "about": plan.about,
            "ref": plan.reference,
            "include": {"raw": true}
        }]
    })
}
