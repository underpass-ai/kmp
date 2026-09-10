use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::plan::KernelWritePlan;

/// Show the compiled direction without repeating canonical refs or proof text.
/// This is a projection of accepted syntax, not a judgment of source fidelity.
pub(super) fn relation_triples(plan: &KernelWritePlan) -> Vec<Value> {
    let local_ids: BTreeMap<_, _> = plan
        .local_refs
        .iter()
        .map(|(id, reference)| (reference.as_str(), id.as_str()))
        .collect();
    let endpoint = |value: &Value| {
        let reference = value.as_str().expect("compiled relation endpoint");
        local_ids
            .get(reference)
            .map_or_else(|| reference.to_owned(), |id| format!("@{id}"))
    };
    plan.ingest_arguments["memory"]["relations"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|relation| {
            json!({
                "from": endpoint(&relation["from"]),
                "rel": relation["rel"],
                "to": endpoint(&relation["to"])
            })
        })
        .collect()
}
