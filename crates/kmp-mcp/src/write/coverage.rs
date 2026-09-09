use serde_json::{Value, json};

use super::{operation::WriteOperation, plan::KernelWritePlan};

/// Coverage of the submitted declaration, never a claim about unseen sources.
/// A summary-only operation preserves memberships; it declares none to add.
pub(crate) fn write_coverage(plan: &KernelWritePlan) -> Value {
    let memory = &plan.ingest_arguments["memory"];
    let entries = memory["entries"].as_array().expect("compiled entries");
    let memberships: usize = entries
        .iter()
        .map(|entry| entry["coordinates"].as_array().map_or(0, Vec::len))
        .sum();
    let is_memory_packet = plan.operation == WriteOperation::Memories;
    json!({
        "scope": "submitted_packet",
        "complete": true,
        "source_coverage": "not_assessed",
        "memories": if is_memory_packet { entries.len() } else { 0 },
        "search_summaries": if is_memory_packet { 0 } else { entries.len() },
        "relations": memory["relations"].as_array().map_or(0, Vec::len),
        "evidence": memory["evidence"].as_array().map_or(0, Vec::len),
        "label_memberships": if is_memory_packet { memberships } else { 0 },
        "preserved_memberships": if is_memory_packet { 0 } else { memberships }
    })
}
