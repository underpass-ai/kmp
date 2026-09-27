use super::validation_error::WriteValidationError;
use serde_json::{Value, json};

use std::collections::BTreeSet;

use kmp_application::{validate_ref_token, validate_supplied_entry_ref};
use kmp_domain::INTENDED_NEW_LABEL_METADATA_KEY;

use super::coordinates::*;
use super::declared_links::DeclaredLinks;
use super::generated_ref::*;
use super::plan::KernelWritePlan;
use super::read_context::ReadContext;
use super::relation_quality::*;
use super::relations::*;
use super::search_summary::decide_search_summary;
use super::validated_arguments::*;
use super::writer_label::*;

const DEFAULT_CONFIDENCE: &str = "high";
const DEFAULT_SOURCE_KIND: &str = "agent";

#[cfg(test)]
pub(crate) fn build_write_plan(arguments: &Value) -> Result<KernelWritePlan, WriteValidationError> {
    build_write_plan_with_root(arguments, false)
}

/// Builds a write plan, allowing the one relation-free strict write that can
/// form a new about's root. The server proves `allow_unlinked_root` by
/// inspecting the about immediately before calling this function; keeping
/// the storage read outside the pure compiler preserves deterministic dry
/// runs and focused validation tests. The first failure stands for them all.
#[cfg(test)]
pub(crate) fn build_write_plan_with_root(
    arguments: &Value,
    allow_unlinked_root: bool,
) -> Result<KernelWritePlan, WriteValidationError> {
    build_write_plan_with_local_refs(arguments, allow_unlinked_root, &BTreeSet::new())
        .map_err(|mut failures| failures.remove(0))
}

/// All batch entry refs are known before any member is compiled. They are
/// current-request context, never a claim that a stored target was inspected.
///
/// A record is judged in one pass: its clock, labels, kind, evidence,
/// rendering, ref and every link are each checked and every failure is
/// returned together, so one resubmission can repair them all. Only what
/// makes the record unreadable — the about, intent, actor, scope or read
/// context — stops the check at once.
pub(super) fn build_write_plan_with_local_refs(
    arguments: &Value,
    allow_unlinked_root: bool,
    batch_refs: &BTreeSet<String>,
) -> Result<KernelWritePlan, Vec<WriteValidationError>> {
    let arguments = arguments.as_object().ok_or_else(|| {
        vec![WriteValidationError::from(
            "tool arguments must be a JSON object",
        )]
    })?;
    let about = required_string(arguments, "about").map_err(|error| vec![error])?;
    validate_ref_token("about", &about)
        .map_err(|error| vec![WriteValidationError::new(error).at("about").global()])?;
    let intent = required_string(arguments, "intent").map_err(|error| vec![error])?;
    validate_intent(&intent).map_err(|error| vec![error.into()])?;
    let actor = required_string(arguments, "actor").map_err(|error| vec![error])?;
    let scope = if batch_refs.is_empty() {
        Some(required_object(arguments, "scope").map_err(|error| vec![error])?)
    } else {
        // Packet records use their explicit memberships without synthesizing
        // a process scope. The single-current contract still requires it.
        arguments.get("scope").and_then(Value::as_object)
    };
    let read_context = ReadContext::from_arguments(arguments)
        .map_err(|error| vec![WriteValidationError::new(error).at("read_context").global()])?;
    let process_scope = scope
        .map(|scope| required_map_string(scope, "process", "scope.process"))
        .transpose()
        .map_err(|error| vec![error])?;
    let current = required_object(arguments, "current").map_err(|error| vec![error])?;

    let mut failures = Vec::new();
    let observed_at = observation_time(arguments).unwrap_or_else(|error| {
        failures.push(error);
        None
    });
    let clocks = WriterCoordinate {
        occurred_at: optional_string(arguments.get("occurred_at")),
        observed_at: observed_at.as_deref(),
        valid_from: optional_string(arguments.get("valid_from")),
        valid_until: optional_string(arguments.get("valid_until")),
        rank: arguments
            .get("rank")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0),
    };
    let task_scope = scope.and_then(|scope| optional_map_string(scope, "task"));
    let episode_scope = scope.and_then(|scope| optional_map_string(scope, "episode"));
    let labels = writer_labels(
        process_scope,
        task_scope,
        episode_scope,
        arguments.get("labels"),
    )
    .unwrap_or_else(|error| {
        failures.push(
            WriteValidationError::new(error)
                .at("labels")
                .code("INVALID_LABELS"),
        );
        Vec::new()
    });
    let options = arguments.get("options").and_then(Value::as_object);
    // A tool called write_memory commits. Previewing was the default here,
    // so every caller that did not know to pass `dry_run: false` got
    // `isError: false` back and wrote nothing — the skill and the write
    // protocol doc both describe the opposite, and `accepted: false` is easy
    // to miss in a tool result. Opt in to the preview, not out of it.
    let dry_run = options
        .and_then(|options| options.get("dry_run"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let strict = options
        .and_then(|options| options.get("strict"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let sequence = options
        .and_then(|options| options.get("sequence"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0);
    let relation_sequence = sequence.unwrap_or(1);
    let labels_new = intended_new_labels(options, &labels).unwrap_or_else(|error| {
        failures.push(error);
        BTreeSet::new()
    });
    // The logical write identity is also the uniqueness component of every
    // generated entry ref. A readable summary slug is useful to humans, but
    // it cannot be an identity: repeated observations legitimately have the
    // same wording, and long summaries routinely share their first line. An
    // exact retry keeps this key (and therefore its refs); a different write
    // gets different refs before it reaches the projection's UPSERT path.
    let idempotency_key = optional_string(arguments.get("idempotency_key"))
        .map(ToString::to_string)
        .unwrap_or_else(|| stable_idempotency_key(arguments));

    let current_kind = required_map_string(current, "kind", "kind")
        .and_then(|kind| {
            validate_node_kind(kind).map(|()| kind).map_err(|error| {
                WriteValidationError::new(error)
                    .at("kind")
                    .code("INVALID_KIND")
                    .allowed_values(crate::contract::writer_memory_kinds::WRITER_MEMORY_KINDS)
            })
        })
        .unwrap_or_else(|error| {
            failures.push(error);
            ""
        });
    let current_summary = required_map_string(current, "summary", "summary")
        .map(Some)
        .unwrap_or_else(|error| {
            failures.push(error);
            None
        });
    let current_evidence = optional_map_string(current, "evidence");
    if strict && current_evidence.is_none() {
        failures.push(
            WriteValidationError::new("strict kmp_write_memory requires evidence")
                .at("evidence")
                .code("MEMORY_EVIDENCE_REQUIRED"),
        );
    }
    // Without a summary there is nothing to judge a rendering against.
    let search_summary = current_summary
        .map(|summary| {
            decide_search_summary(summary, optional_map_string(current, "summary_en"), strict)
        })
        .transpose()
        .unwrap_or_else(|error| {
            failures.push(error);
            None
        })
        .unwrap_or_default();
    let current_summary = current_summary.unwrap_or_default();

    let current_ref = match optional_map_string(current, "ref") {
        Some(current_ref) => {
            if let Err(error) = validate_supplied_entry_ref(&about, "ref", current_ref) {
                failures.push(
                    WriteValidationError::new(error)
                        .at("ref")
                        .code("INVALID_REF"),
                );
            }
            current_ref.to_string()
        }
        None => generated_entry_ref(
            &about,
            current_kind,
            current_summary,
            &idempotency_key,
            "current",
        ),
    };
    let mut generated_refs = vec![current_ref.clone()];
    let mut local_refs = std::borrow::Cow::Borrowed(batch_refs);
    if !local_refs.contains(&current_ref) {
        local_refs.to_mut().insert(current_ref.clone());
    }
    let mut dimensions = Vec::new();
    let mut coordinates = Vec::new();
    for label in &labels {
        match kmp_domain::MemoryDimensionIdentity::new(&about, &label.key, &label.value) {
            Ok(identity) => {
                let scope_id = identity.node_id();
                let mut declared = dimension(&scope_id, &label.key, label.title);
                if labels_new.contains(&label.key) {
                    declared["metadata"] = json!({ INTENDED_NEW_LABEL_METADATA_KEY: "true" });
                }
                dimensions.push(declared);
                coordinates.push(coordinate(&label.key, &scope_id, sequence, clocks));
            }
            Err(error) => failures.push(
                WriteValidationError::new(error.to_string())
                    .at(label.field.clone())
                    .code("INVALID_LABELS"),
            ),
        }
    }

    let mut current_metadata = json!({
        "writer_intent": intent,
        "writer_actor": actor
    });
    if let Some(summary) = &search_summary.stored {
        current_metadata["summary_en"] = json!(summary);
    }
    let mut entries = vec![json!({
        "id": current_ref.clone(),
        "kind": current_kind,
        "text": current_summary,
        "coordinates": coordinates.clone(),
        "metadata": current_metadata
    })];
    let mut relations = Vec::new();
    let mut relation_names = Vec::new();
    let mut relation_quality = Vec::new();
    let mut evidence = Vec::new();
    if let Some(current_evidence) = current_evidence {
        evidence.push(json!({
            "id": format!("evidence:{}:current", current_ref),
            "supports": [current_ref.clone()],
            "text": current_evidence,
            "source": format!("kmp_write_memory:{actor}"),
            "time": observed_at
        }));
    }

    let connect_to =
        optional_array(arguments.get("connect_to"), "connect_to").unwrap_or_else(|error| {
            failures.push(WriteValidationError::new(error).at("connect_to"));
            &[]
        });
    if strict && connect_to.is_empty() && !allow_unlinked_root {
        failures.push(WriteValidationError::new("strict kmp_write_memory requires at least one connect_to relation once the about exists; inspect or traverse a target first, or set options.strict=false when an unlinked write is intentional"));
    }
    let links = DeclaredLinks {
        about: &about,
        from: &current_ref,
        actor: &actor,
        observed_at: observed_at.as_deref(),
        strict,
        sequence: relation_sequence,
        read_context: &read_context,
        local_refs: &local_refs,
    };
    for (index, link) in connect_to.iter().enumerate() {
        match links.compile(index, link) {
            Ok(compiled) => {
                relations.push(compiled.relation);
                relation_names.push(compiled.name);
                relation_quality.push(compiled.quality);
                evidence.extend(compiled.evidence);
            }
            Err(error) => failures.push(error),
        }
    }

    if let Some(delta) = arguments.get("semantic_delta").and_then(Value::as_object) {
        let delta = (|| {
            let delta_from = required_map_string(delta, "from", "semantic_delta.from")?;
            let delta_to = required_map_string(delta, "to", "semantic_delta.to")?;
            let delta_why = required_map_string(delta, "why", "semantic_delta.why")?;
            let delta_evidence = required_map_string(delta, "evidence", "semantic_delta.evidence")?;
            let delta_ref = if let Some(delta_ref) = optional_map_string(delta, "ref") {
                validate_supplied_entry_ref(&about, "semantic_delta.ref", delta_ref)?;
                delta_ref.to_string()
            } else {
                generated_entry_ref(
                    &about,
                    "semantic_delta",
                    delta_to,
                    &idempotency_key,
                    "semantic_delta",
                )
            };
            reject_duplicate_ref(&mut generated_refs, &delta_ref)?;
            local_refs.to_mut().insert(delta_ref.clone());
            entries.push(json!({
                "id": delta_ref.clone(),
                "kind": "semantic_delta",
                "text": format!("From: {delta_from}\nTo: {delta_to}\nWhy: {delta_why}"),
                "coordinates": shifted_coordinates(&entries[0]["coordinates"], 1),
                "metadata": {
                    "writer_intent": intent,
                    "writer_actor": actor,
                    "delta_from": delta_from,
                    "delta_to": delta_to
                }
            }));
            let updates_state_quality = relation_quality_diagnostic(RelationQualityInput {
                about: &about,
                from: &current_ref,
                to: &delta_ref,
                rel: "updates_state",
                semantic_class: "causal",
                confidence: DEFAULT_CONFIDENCE,
                why: delta_why,
                evidence: delta_evidence,
                strict,
                read_context: &read_context,
                local_refs: &local_refs,
            })?;
            relations.push(relation(
                &current_ref,
                &delta_ref,
                "updates_state",
                "causal",
                DEFAULT_CONFIDENCE,
                delta_why,
                delta_evidence,
                relation_sequence.saturating_add(1),
            ));
            relation_names.push("updates_state".to_string());
            relation_quality.push(updates_state_quality);
            if let Some(first_link) = connect_to.first().and_then(Value::as_object) {
                let target_ref = required_map_string(first_link, "ref", "connect_to[0].ref")?;
                let semantic_delta_quality = relation_quality_diagnostic(RelationQualityInput {
                    about: &about,
                    from: &delta_ref,
                    to: target_ref,
                    rel: "semantic_delta_from",
                    semantic_class: "causal",
                    confidence: DEFAULT_CONFIDENCE,
                    why: delta_why,
                    evidence: delta_evidence,
                    strict,
                    read_context: &read_context,
                    local_refs: &local_refs,
                })?;
                relations.push(relation(
                    &delta_ref,
                    target_ref,
                    "semantic_delta_from",
                    "causal",
                    DEFAULT_CONFIDENCE,
                    delta_why,
                    delta_evidence,
                    relation_sequence.saturating_add(1),
                ));
                relation_names.push("semantic_delta_from".to_string());
                relation_quality.push(semantic_delta_quality);
            }
            evidence.push(json!({
                "id": format!("evidence:{}:semantic_delta", delta_ref),
                "supports": [delta_ref.clone(), current_ref.clone()],
                "text": delta_evidence,
                "source": format!("kmp_write_memory:{actor}:semantic_delta"),
                "time": observed_at
            }));
            Ok::<(), WriteValidationError>(())
        })();
        if let Err(error) = delta {
            failures.push(error);
        }
    }
    if !failures.is_empty() {
        return Err(failures);
    }

    let mut ingest_arguments = json!({
        "about": about.clone(),
        "idempotency_key": idempotency_key.clone(),
        "dry_run": dry_run,
        "default_observation_to_ingestion": true,
        "label_policy": if strict { "refuse" } else { "warn" },
        "memory": {
            "dimensions": dimensions,
            "entries": entries,
            "relations": relations,
            "evidence": evidence
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
    let relation_quality_metrics = relation_quality_metrics(&relation_quality);

    super::coordinates::omit_unknown_observations(&mut ingest_arguments);
    Ok(KernelWritePlan {
        operation: super::operation::WriteOperation::Memories,
        about,
        local_refs: Default::default(),
        dry_run,
        ingest_arguments,
        generated_refs,
        labels: labels
            .iter()
            .map(|label| json!({ "key": label.key, "value": label.value }))
            .collect(),
        relations: relation_names,
        relation_quality,
        relation_quality_metrics,
        replaced: Vec::new(),
        diagnostics: search_summary.diagnostics,
        next_suggested_reads: suggested_reads(&current_ref, connect_to),
    })
}

/// The label keys a writer who read the catalogue insists are new. The
/// kernel then leaves those labels out of the resemblance check instead of
/// refusing or warning. Every key must be one this write declares.
fn intended_new_labels(
    options: Option<&serde_json::Map<String, Value>>,
    labels: &[WriterLabel],
) -> Result<BTreeSet<String>, WriteValidationError> {
    let Some(value) = options.and_then(|options| options.get("labels_new")) else {
        return Ok(BTreeSet::new());
    };
    let shape = || {
        WriteValidationError::new("options.labels_new must be an array of label keys")
            .at("options.labels_new")
    };
    let keys = value
        .as_array()
        .ok_or_else(shape)?
        .iter()
        .map(|key| key.as_str().map(str::to_string).ok_or_else(shape))
        .collect::<Result<BTreeSet<_>, _>>()?;
    // Without labels the missing membership is the failure to report.
    if labels.is_empty() {
        return Ok(keys);
    }
    for key in &keys {
        if !labels.iter().any(|label| label.key == *key) {
            return Err(WriteValidationError::new(format!(
                "options.labels_new names `{key}`, which is not a label of this write"
            )));
        }
    }
    Ok(keys)
}

fn dimension(id: &str, kind: &str, title: &str) -> Value {
    json!({
        "id": id,
        "kind": kind,
        "title": title
    })
}

fn suggested_reads(current_ref: &str, connect_to: &[Value]) -> Vec<Value> {
    connect_to
        .first()
        .and_then(Value::as_object)
        .and_then(|link| link.get("ref"))
        .and_then(Value::as_str)
        .map(|target_ref| {
            vec![json!({
                "tool": "kmp_trace",
                "from": current_ref,
                "to": target_ref
            })]
        })
        .unwrap_or_default()
}
