//! The write that attaches an English search summary to a memory that
//! already exists: `kmp_write_memory` with the intent `record_summary`.
//!
//! It compiles to an ingest of the same entry — same ref, same kind, same
//! text, same coordinates — with `summary_en` added to its metadata. The
//! event log keeps the write as what it is, a rendering attached later by a
//! named writer, and the projection reads the summary from then on. Nothing
//! else about the memory can change through this path, because nothing else
//! is taken from the caller.

use serde_json::{Value, json};

use super::arguments::*;
use super::coordinates::reject_a_time_that_has_not_happened;
use super::existing_entry::ExistingEntry;
use super::generated_ref::stable_idempotency_key;
use super::plan::KernelWritePlan;
use super::relation_quality::relation_quality_metrics;
use super::search_summary::decide_search_summary;

const DEFAULT_SOURCE_KIND: &str = "agent";

/// Whether the arguments ask for this planner rather than the general one.
pub(crate) fn is_summary_write(arguments: &Value) -> bool {
    arguments
        .get("intent")
        .and_then(Value::as_str)
        .is_some_and(|intent| intent == "record_summary")
}

/// The ref a `record_summary` write attaches to, so the server can read it
/// before planning.
pub(crate) fn summary_target(arguments: &Value) -> Result<(String, String), String> {
    let arguments = arguments
        .as_object()
        .ok_or_else(|| "tool arguments must be a JSON object".to_string())?;
    let about = required_string(arguments, "about")?;
    let current = required_object(arguments, "current")?;
    let reference = required_map_string(current, "ref", "current.ref")?;
    kmp_application::validate_supplied_entry_ref(&about, "current.ref", reference)?;
    Ok((about, reference.to_string()))
}

pub(crate) fn build_summary_plan(
    arguments: &Value,
    existing: &ExistingEntry,
) -> Result<KernelWritePlan, String> {
    let arguments = arguments
        .as_object()
        .ok_or_else(|| "tool arguments must be a JSON object".to_string())?;
    let about = required_string(arguments, "about")?;
    let actor = required_string(arguments, "actor")?;
    let observed_at = required_string(arguments, "observed_at")?;
    reject_a_time_that_has_not_happened(&observed_at, crate::clock::now_seconds())?;
    let current = required_object(arguments, "current")?;
    let summary = required_map_string(current, "summary_en", "current.summary_en")?;
    for field in ["summary", "kind", "evidence"] {
        if current.contains_key(field) {
            return Err(format!(
                "record_summary attaches a rendering to a memory that exists; current.{field} is \
                 read from the store and cannot be supplied"
            ));
        }
    }
    if arguments
        .get("connect_to")
        .and_then(Value::as_array)
        .is_some_and(|links| !links.is_empty())
        || arguments.get("semantic_delta").is_some()
    {
        return Err(
            "record_summary writes no relation and no delta: attach the summary on its own"
                .to_string(),
        );
    }
    let options = arguments.get("options").and_then(Value::as_object);
    let dry_run = options
        .and_then(|options| options.get("dry_run"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let strict = options
        .and_then(|options| options.get("strict"))
        .and_then(Value::as_bool)
        .unwrap_or(true);

    // The one judgement this write makes, and the same one the ingest and
    // the ranker make: a summary that will not carry is refused here, while
    // the writer can still fix it.
    let decision = decide_search_summary(&existing.text, Some(summary), strict)?;
    let Some(stored) = decision.stored else {
        return Err("record_summary requires current.summary_en".to_string());
    };

    let mut metadata = existing.metadata.clone();
    metadata.insert("summary_en".to_string(), json!(stored));
    metadata.insert("summary_en_by".to_string(), json!(actor));
    let idempotency_key = optional_string(arguments.get("idempotency_key"))
        .map(ToString::to_string)
        .unwrap_or_else(|| stable_idempotency_key(arguments));

    let ingest_arguments = json!({
        "about": about.clone(),
        "idempotency_key": idempotency_key.clone(),
        "dry_run": dry_run,
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

    let mut diagnostics = decision.diagnostics;
    diagnostics.push(format!(
        "attached summary_en to `{}`; its text, kind and coordinates are the stored ones",
        existing.reference
    ));
    Ok(KernelWritePlan {
        about,
        dry_run,
        ingest_arguments,
        generated_refs: Vec::new(),
        relations: Vec::new(),
        relation_quality: Vec::new(),
        relation_quality_metrics: relation_quality_metrics(&[]),
        diagnostics,
        next_suggested_reads: vec![json!({
            "tool": "kmp_inspect",
            "about": existing.reference.split(':').take(2).collect::<Vec<_>>().join(":"),
            "ref": existing.reference
        })],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    fn existing() -> ExistingEntry {
        ExistingEntry {
            reference: "project:kmp:decision:valkey".to_string(),
            kind: "decision".to_string(),
            text: "Se adoptó Valkey 7.2 para el almacén compartido (ADR-018).".to_string(),
            coordinates: vec![json!({
                "dimension": "work",
                "scope_id": "about:project:kmp:dimension:work:main",
                "occurred_at": "2026-05-06T10:00:00Z",
                "ingested_at": "2026-05-06T10:00:01Z",
                "sequence": 3
            })],
            metadata: Map::from_iter([("writer_actor".to_string(), json!("agent:a"))]),
        }
    }

    fn request(summary_en: &str) -> Value {
        json!({
            "about": "project:kmp",
            "intent": "record_summary",
            "actor": "agent:b",
            "observed_at": "2026-05-07T10:00:00Z",
            "scope": {"process": "project:kmp:backfill"},
            "current": {"ref": "project:kmp:decision:valkey", "summary_en": summary_en}
        })
    }

    #[test]
    fn a_faithful_rendering_is_written_back_onto_the_stored_entry_unchanged() {
        let plan = build_summary_plan(
            &request("Valkey 7.2 was adopted for the shared store (ADR-018)."),
            &existing(),
        )
        .expect("a faithful rendering attaches");

        let entry = &plan.ingest_arguments["memory"]["entries"][0];
        assert_eq!(entry["id"], "project:kmp:decision:valkey");
        assert_eq!(entry["kind"], "decision");
        assert_eq!(
            entry["text"],
            "Se adoptó Valkey 7.2 para el almacén compartido (ADR-018)."
        );
        assert_eq!(entry["coordinates"][0]["sequence"], 3);
        assert_eq!(
            entry["metadata"]["summary_en"],
            "Valkey 7.2 was adopted for the shared store (ADR-018)."
        );
        assert_eq!(entry["metadata"]["summary_en_by"], "agent:b");
        assert_eq!(entry["metadata"]["writer_actor"], "agent:a");
        assert!(plan.generated_refs.is_empty());
        assert!(plan.relations.is_empty());
        assert!(
            plan.diagnostics[0].contains("attached summary_en"),
            "{:?}",
            plan.diagnostics
        );
        assert_eq!(plan.ingest_arguments["memory"]["dimensions"], json!([]));
    }

    #[test]
    fn a_rendering_that_fails_the_lint_is_refused_with_the_fault_named() {
        let error = build_summary_plan(
            &request("Valkey was adopted for the shared store."),
            &existing(),
        )
        .expect_err("a dropped identifier is refused");

        assert!(error.contains("refuses current.summary_en"), "{error}");
        assert!(error.contains("7.2"), "{error}");
        assert!(error.contains("adr-018"), "{error}");
    }

    #[test]
    fn the_stored_text_kind_and_links_cannot_be_supplied() {
        let mut with_text = request("Valkey 7.2 was adopted for the shared store (ADR-018).");
        with_text["current"]["summary"] = json!("something else");
        let error = build_summary_plan(&with_text, &existing()).expect_err("text is the store's");
        assert!(
            error.contains("current.summary is read from the store"),
            "{error}"
        );

        let mut with_link = request("Valkey 7.2 was adopted for the shared store (ADR-018).");
        with_link["connect_to"] =
            json!([{"ref": "project:kmp:x", "rel": "follows", "class": "procedural"}]);
        let error = build_summary_plan(&with_link, &existing()).expect_err("no relations");
        assert!(error.contains("writes no relation"), "{error}");
    }

    #[test]
    fn the_target_is_read_from_the_arguments_and_must_belong_to_the_about() {
        let (about, reference) =
            summary_target(&request("x")).expect("about and ref are in the arguments");
        assert_eq!(about, "project:kmp");
        assert_eq!(reference, "project:kmp:decision:valkey");

        let mut foreign = request("x");
        foreign["current"]["ref"] = json!("project:other:decision:valkey");
        assert!(summary_target(&foreign).is_err());
    }
}
