//! What a relation-only write created, said apart from what it left alone.
//!
//! A caller reading `accepted: true` cannot otherwise tell an attachment
//! from a replacement: both answer with entries, relations and evidence. So
//! this names the two halves out loud — the relation and the evidence node
//! this write brought into existence, and the source memories it wrote back
//! unchanged — and refuses to guess for any other operation.

use serde_json::{Value, json};

use super::operation::WriteOperation;
use super::plan::KernelWritePlan;

pub(crate) fn attachment(plan: &KernelWritePlan) -> Option<Value> {
    if plan.operation != WriteOperation::Relations {
        return None;
    }
    let memory = &plan.ingest_arguments["memory"];
    let evidence = memory["evidence"].as_array()?;
    let relations = memory["relations"]
        .as_array()?
        .iter()
        .zip(evidence)
        .map(|(relation, evidence)| {
            json!({
                "from": relation["from"],
                "rel": relation["rel"],
                "to": relation["to"],
                "class": relation["class"],
                "confidence": relation["confidence"],
                // Absent means the kernel stamps its ingestion instant; it
                // never means an endpoint's observation.
                "observed_at": relation["clocks"]["observed_at"],
                "evidence_ref": evidence["id"]
            })
        })
        .collect::<Vec<_>>();
    Some(json!({
        "created": {
            "relations": relations,
            "evidence": evidence
                .iter()
                .map(|item| item["id"].clone())
                .collect::<Vec<_>>()
        },
        "unchanged_sources": memory["entries"]
            .as_array()?
            .iter()
            .map(|entry| entry["id"].clone())
            .collect::<Vec<_>>()
    }))
}
