use serde_json::{Value, json};

use kmp_proto::v1beta1::IngestResponse;

use super::wording::plural;

use crate::write::KmpIngestPlan;

pub(crate) fn ingest_from_response(response: IngestResponse) -> Value {
    let memory = response.memory.as_ref();
    let accepted = memory.and_then(|memory| memory.accepted.as_ref());

    json!({
        "summary": response.summary,
        "memory": {
            "about": memory.map(|memory| memory.about.as_str()).unwrap_or(""),
            "memory_id": memory.map(|memory| memory.memory_id.as_str()).unwrap_or(""),
            "accepted": {
                "entries": accepted.map(|accepted| accepted.entries).unwrap_or_default(),
                "relations": accepted.map(|accepted| accepted.relations).unwrap_or_default(),
                "evidence": accepted.map(|accepted| accepted.evidence).unwrap_or_default()
            },
            "read_after_write_ready": memory
                .map(|memory| memory.read_after_write_ready)
                .unwrap_or(false),
            "created_dimensions": memory
                .map(|memory| memory.created_dimensions.clone())
                .unwrap_or_default()
        },
        "warnings": response.warnings
    })
}

pub(crate) fn dry_run_ingest_from_plan(plan: &KmpIngestPlan) -> Value {
    json!({
        "summary": format!(
            "Ingested {} {}, {} {}, and {} {} for {}.",
            plan.accepted.entries,
            plural(plan.accepted.entries, "entry", "entries"),
            plan.accepted.relations,
            plural(plan.accepted.relations, "relation", "relations"),
            plan.accepted.evidence,
            plural(plan.accepted.evidence, "evidence item", "evidence items"),
            plan.about
        ),
        "memory": {
            "about": plan.about,
            "memory_id": plan.memory_id,
            "accepted": {
                "entries": plan.accepted.entries,
                "relations": plan.accepted.relations,
                "evidence": plan.accepted.evidence
            },
            "read_after_write_ready": false,
            "created_dimensions": []
        },
        "warnings": [
            "dry_run=true; validated memory without sending a KernelMemoryService.Ingest call"
        ]
    })
}
