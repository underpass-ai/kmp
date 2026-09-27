//! The write that attaches judged search expansions to a memory that
//! already exists: `kmp_write_memory` `search_summaries[]` with
//! `search_expansions` (P15).
//!
//! Like a summary, it compiles to an ingest of the same entry — same ref,
//! kind, text and coordinates — with only its metadata changed. Nothing
//! else about the memory is taken from the caller.

use serde_json::{Map, Value, json};

use super::coordinates::observation_time;
use super::existing_entry::ExistingEntry;
use super::generated_ref::stable_idempotency_key;
use super::plan::KernelWritePlan;
use super::relation_quality::relation_quality_metrics;
use super::validated_arguments::{optional_string, required_string};
use super::validation_error::WriteValidationError;

const DEFAULT_SOURCE_KIND: &str = "agent";

/// A plan that writes `existing` back with `expansions` (the kept
/// expansions' metadata) added to what it already carries.
pub(crate) fn build_expansion_plan(
    arguments: &Value,
    existing: &ExistingEntry,
    expansions: &Map<String, Value>,
) -> Result<KernelWritePlan, WriteValidationError> {
    let arguments = arguments
        .as_object()
        .ok_or_else(|| "tool arguments must be a JSON object".to_string())?;
    let about = required_string(arguments, "about")?;
    let actor = required_string(arguments, "actor")?;
    let observed_at = observation_time(arguments)?;
    let dry_run = arguments
        .get("options")
        .and_then(|options| options.get("dry_run"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut metadata = existing.metadata.clone();
    metadata.extend(expansions.clone());
    let idempotency_key = optional_string(arguments.get("idempotency_key"))
        .map(ToString::to_string)
        .unwrap_or_else(|| stable_idempotency_key(arguments));
    let mut ingest_arguments = json!({
        "about": about.clone(),
        "idempotency_key": idempotency_key.clone(),
        "dry_run": dry_run,
        "default_observation_to_ingestion": true,
        "memory": {
            "dimensions": [],
            "entries": [{
                "id": existing.reference,
                "kind": existing.kind,
                "text": existing.text,
                "coordinates": existing.coordinates,
                "metadata": metadata
            }],
            "relations": [],
            "evidence": []
        },
        "provenance": {
            "source_kind": arguments
                .get("source_kind")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_SOURCE_KIND),
            "source_agent": actor,
            "observed_at": observed_at,
            "correlation_id": format!("kmp_write:{about}"),
            "causation_id": idempotency_key
        }
    });
    super::coordinates::omit_unknown_observations(&mut ingest_arguments);
    Ok(KernelWritePlan {
        operation: super::operation::WriteOperation::SearchSummaries,
        about,
        local_refs: Default::default(),
        dry_run,
        ingest_arguments,
        generated_refs: Vec::new(),
        labels: Vec::new(),
        relations: Vec::new(),
        relation_quality: Vec::new(),
        relation_quality_metrics: relation_quality_metrics(&[]),
        replaced: Vec::new(),
        diagnostics: vec![format!(
            "attached judged search expansions to `{}`; its text, kind and coordinates are the stored ones",
            existing.reference
        )],
        next_suggested_reads: Vec::new(),
    })
}

/// Adds the kept expansions' metadata to the one entry a summary plan
/// writes back.
pub(crate) fn attach_expansions(plan: &mut KernelWritePlan, expansions: &Map<String, Value>) {
    if let Some(Value::Object(metadata)) = plan
        .ingest_arguments
        .pointer_mut("/memory/entries/0/metadata")
    {
        metadata.extend(expansions.clone());
    }
    plan.diagnostics
        .push("attached judged search expansions beside the summary".to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn existing() -> ExistingEntry {
        ExistingEntry {
            reference: "project:kmp:decision:rollout".to_string(),
            revision: 3,
            kind: "decision".to_string(),
            text: "The rollout slipped because the auditors had not signed off.".to_string(),
            coordinates: vec![json!({
                "dimension": "work",
                "scope_id": "label:v1:project%3Akmp:work:work%3Amain",
                "occurred_at": "2026-05-06T10:00:00Z",
                "ingested_at": "2026-05-06T10:00:01Z",
                "sequence": 3
            })],
            metadata: Map::from_iter([("writer_actor".to_string(), json!("agent:a"))]),
        }
    }

    fn expansions() -> Map<String, Value> {
        Map::from_iter([(
            "search_expansions".to_string(),
            json!("Why was the launch postponed?"),
        )])
    }

    #[test]
    fn the_stored_entry_is_written_back_with_only_its_expansions_added() {
        let plan = build_expansion_plan(
            &json!({"about": "project:kmp", "actor": "agent:b"}),
            &existing(),
            &expansions(),
        )
        .expect("plan");
        let entry = &plan.ingest_arguments["memory"]["entries"][0];
        assert_eq!(entry["id"], "project:kmp:decision:rollout");
        assert_eq!(entry["text"], existing().text);
        assert_eq!(entry["coordinates"][0]["sequence"], 3);
        assert_eq!(entry["metadata"]["writer_actor"], "agent:a");
        assert_eq!(
            entry["metadata"]["search_expansions"],
            "Why was the launch postponed?"
        );
        assert!(plan.generated_refs.is_empty());
        assert!(plan.diagnostics[0].contains("judged search expansions"));
        assert!(
            build_expansion_plan(&json!({"about": "project:kmp"}), &existing(), &expansions())
                .is_err(),
            "an actor is required"
        );
    }

    #[test]
    fn a_summary_plan_gains_the_expansions_beside_its_summary() {
        let mut plan = build_expansion_plan(
            &json!({"about": "project:kmp", "actor": "agent:b"}),
            &existing(),
            &Map::new(),
        )
        .expect("plan");
        attach_expansions(&mut plan, &expansions());
        assert_eq!(
            plan.ingest_arguments["memory"]["entries"][0]["metadata"]["search_expansions"],
            "Why was the launch postponed?"
        );
    }
}
