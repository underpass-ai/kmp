use super::validation_error::WriteValidationError;
use serde_json::{Value, json};

use std::collections::BTreeSet;

use kmp_application::{validate_ref_token, validate_supplied_entry_ref};
use kmp_domain::{INTENDED_NEW_LABEL_METADATA_KEY, MemoryRelationType};

use super::coordinates::*;
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
/// runs and focused validation tests.
#[cfg(test)]
pub(crate) fn build_write_plan_with_root(
    arguments: &Value,
    allow_unlinked_root: bool,
) -> Result<KernelWritePlan, WriteValidationError> {
    build_write_plan_with_local_refs(arguments, allow_unlinked_root, &BTreeSet::new())
}

/// All batch entry refs are known before any member is compiled. They are
/// current-request context, never a claim that a stored target was inspected.
pub(super) fn build_write_plan_with_local_refs(
    arguments: &Value,
    allow_unlinked_root: bool,
    batch_refs: &BTreeSet<String>,
) -> Result<KernelWritePlan, WriteValidationError> {
    let arguments = arguments
        .as_object()
        .ok_or_else(|| "tool arguments must be a JSON object".to_string())?;
    let about = required_string(arguments, "about")?;
    validate_ref_token("about", &about)
        .map_err(|error| WriteValidationError::new(error).at("about").global())?;
    let intent = required_string(arguments, "intent")?;
    validate_intent(&intent)?;
    let actor = required_string(arguments, "actor")?;
    let observed_at = required_string(arguments, "observed_at")?;
    reject_a_time_that_has_not_happened(&observed_at, crate::clock::now_seconds()).map_err(
        |error| {
            WriteValidationError::new(error)
                .at("observed_at")
                .code("FUTURE_OBSERVATION")
        },
    )?;
    let clocks = WriterCoordinate {
        occurred_at: optional_string(arguments.get("occurred_at")),
        observed_at: &observed_at,
        valid_from: optional_string(arguments.get("valid_from")),
        valid_until: optional_string(arguments.get("valid_until")),
        rank: arguments
            .get("rank")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0),
    };
    let scope = if batch_refs.is_empty() {
        Some(required_object(arguments, "scope")?)
    } else {
        // Packet records use their explicit memberships without synthesizing
        // a process scope. The single-current contract still requires it.
        arguments.get("scope").and_then(Value::as_object)
    };
    let read_context = ReadContext::from_arguments(arguments)
        .map_err(|error| WriteValidationError::new(error).at("read_context").global())?;
    let process_scope = scope
        .map(|scope| required_map_string(scope, "process", "scope.process"))
        .transpose()?;
    let task_scope = scope.and_then(|scope| optional_map_string(scope, "task"));
    let episode_scope = scope.and_then(|scope| optional_map_string(scope, "episode"));
    let labels = writer_labels(
        process_scope,
        task_scope,
        episode_scope,
        arguments.get("labels"),
    )
    .map_err(|error| {
        WriteValidationError::new(error)
            .at("labels")
            .code("INVALID_LABELS")
    })?;
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
    // A writer that read the catalogue and still means a new label says so
    // per key; the kernel then leaves that label out of the resemblance
    // check instead of refusing or warning.
    let labels_new = options
        .and_then(|options| options.get("labels_new"))
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| "options.labels_new must be an array of label keys".to_string())
                .and_then(|keys| {
                    keys.iter()
                        .map(|key| {
                            key.as_str().map(str::to_string).ok_or_else(|| {
                                "options.labels_new must be an array of label keys".to_string()
                            })
                        })
                        .collect::<Result<BTreeSet<_>, _>>()
                })
        })
        .transpose()?
        .unwrap_or_default();
    for key in &labels_new {
        if !labels.iter().any(|label| label.key == *key) {
            return Err(WriteValidationError::new(format!(
                "options.labels_new names `{key}`, which is not a label of this write"
            )));
        }
    }
    // The logical write identity is also the uniqueness component of every
    // generated entry ref. A readable summary slug is useful to humans, but
    // it cannot be an identity: repeated observations legitimately have the
    // same wording, and long summaries routinely share their first line. An
    // exact retry keeps this key (and therefore its refs); a different write
    // gets different refs before it reaches the projection's UPSERT path.
    let idempotency_key = optional_string(arguments.get("idempotency_key"))
        .map(ToString::to_string)
        .unwrap_or_else(|| stable_idempotency_key(arguments));

    let current = required_object(arguments, "current")?;
    let current_kind = required_map_string(current, "kind", "kind")?;
    validate_node_kind(current_kind).map_err(|error| {
        WriteValidationError::new(error)
            .at("kind")
            .code("INVALID_KIND")
            .allowed_values(crate::contract::writer_memory_kinds::WRITER_MEMORY_KINDS)
    })?;
    let current_summary = required_map_string(current, "summary", "summary")?;
    let current_evidence = optional_map_string(current, "evidence");
    if strict && current_evidence.is_none() {
        return Err(
            WriteValidationError::new("strict kmp_write_memory requires evidence")
                .at("evidence")
                .code("MEMORY_EVIDENCE_REQUIRED"),
        );
    }
    let search_summary = decide_search_summary(
        current_summary,
        optional_map_string(current, "summary_en"),
        strict,
    )?;

    let current_ref = if let Some(current_ref) = optional_map_string(current, "ref") {
        validate_supplied_entry_ref(&about, "ref", current_ref).map_err(|error| {
            WriteValidationError::new(error)
                .at("ref")
                .code("INVALID_REF")
        })?;
        current_ref.to_string()
    } else {
        generated_entry_ref(
            &about,
            current_kind,
            current_summary,
            &idempotency_key,
            "current",
        )
    };
    let mut generated_refs = vec![current_ref.clone()];
    let mut local_refs = std::borrow::Cow::Borrowed(batch_refs);
    if !local_refs.contains(&current_ref) {
        local_refs.to_mut().insert(current_ref.clone());
    }
    let mut dimensions = Vec::new();
    let mut coordinates = Vec::new();
    for label in &labels {
        let scope_id = kmp_domain::MemoryDimensionIdentity::new(&about, &label.key, &label.value)
            .map_err(|error| error.to_string())?
            .node_id();
        let mut declared = dimension(&scope_id, &label.key, label.title);
        if labels_new.contains(&label.key) {
            declared["metadata"] = json!({ INTENDED_NEW_LABEL_METADATA_KEY: "true" });
        }
        dimensions.push(declared);
        coordinates.push(coordinate(&label.key, &scope_id, sequence, clocks));
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

    let connect_to = optional_array(arguments.get("connect_to"), "connect_to")?;
    if strict && connect_to.is_empty() && !allow_unlinked_root {
        return Err(WriteValidationError::new("strict kmp_write_memory requires at least one connect_to relation once the about exists; inspect or traverse a target first, or set options.strict=false when an unlinked write is intentional"
                .to_string()));
    }
    for (index, link) in connect_to.iter().enumerate() {
        let link = link
            .as_object()
            .ok_or_else(|| format!("connect_to[{index}] must be an object"))?;
        let target_ref = required_map_string(link, "ref", &format!("connect_to[{index}].ref"))?;
        let rel_arg = required_map_string(link, "rel", &format!("connect_to[{index}].rel"))?;
        let relation_type = MemoryRelationType::new(rel_arg)
            .map_err(|error| format!("connect_to[{index}].rel is invalid: {error}"))?;
        let rel = relation_type.as_str();
        let semantic_class = resolve_class(link, rel, strict)
            .map_err(|error| error.within(&format!("connect_to[{index}]")))?;
        validate_semantic_class(semantic_class).map_err(|error| {
            WriteValidationError::new(error).at(format!("connect_to[{index}].class"))
        })?;
        let why = required_relation_string(link, "why", semantic_class, index)?;
        let relation_evidence = required_relation_string(link, "evidence", semantic_class, index)?;
        let confidence = optional_map_string(link, "confidence").unwrap_or(DEFAULT_CONFIDENCE);
        validate_confidence(confidence).map_err(|error| {
            WriteValidationError::new(error).at(format!("connect_to[{index}].confidence"))
        })?;
        let quality = relation_quality_diagnostic(RelationQualityInput {
            about: &about,
            from: &current_ref,
            to: target_ref,
            rel,
            semantic_class,
            confidence,
            why,
            evidence: relation_evidence,
            strict,
            read_context: &read_context,
            local_refs: &local_refs,
        })
        .map_err(|error| error.within(&format!("connect_to[{index}]")))?;

        let mut link_value = relation(
            &current_ref,
            target_ref,
            rel,
            semantic_class,
            confidence,
            why,
            relation_evidence,
            relation_sequence,
        );
        // An equivalence across abouts carries the proposal it was declared
        // from as its method, which is what the kernel admits it by.
        let crosses_about = quality["crosses_about"] == true;
        if crosses_about {
            let proposal = read_context
                .relate_proposal_for(&about, target_ref)
                .ok_or_else(|| "cross-about equivalence without its proposal".to_string())?;
            link_value["method"] = json!(format!(
                "{}:{}",
                kmp_domain::DECLARED_FROM_RELATE_METHOD,
                proposal.proposed_by.join("+")
            ));
        }
        relations.push(link_value);
        relation_names.push(rel.to_string());
        relation_quality.push(quality);
        // A structural link is exempt from evidence, and an evidence item with
        // no text is not evidence: the canonical ingest mapper requires
        // `memory.evidence[].text`, and rightly refuses an empty one. The
        // evidence node supports what this about owns; a ref of another
        // about is named by the relation, never claimed by the evidence.
        if !relation_evidence.trim().is_empty() {
            let supports = if crosses_about {
                json!([current_ref.clone()])
            } else {
                json!([current_ref.clone(), target_ref])
            };
            evidence.push(json!({
                "id": format!("evidence:{}:relation:{}", current_ref, index + 1),
                "supports": supports,
                "text": relation_evidence,
                "source": format!("kmp_write_memory:{actor}:relation:{rel}"),
                "time": observed_at
            }));
        }
    }

    if let Some(delta) = arguments.get("semantic_delta").and_then(Value::as_object) {
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
    }

    let ingest_arguments = json!({
        "about": about.clone(),
        "idempotency_key": idempotency_key.clone(),
        "dry_run": dry_run,
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
        diagnostics: search_summary.diagnostics,
        next_suggested_reads: suggested_reads(&current_ref, connect_to),
    })
}

/// The labels a write emits, in the order the ingest has always carried
/// them: the well-known task, process and episode scopes first, then the
/// caller's own `labels` by key. Packet records can supply only labels;
/// the single-current form provides the well-known process/task/episode
/// memberships through scope. Neither path invents a label.
fn writer_labels(
    process: Option<&str>,
    task: Option<&str>,
    episode: Option<&str>,
    labels: Option<&Value>,
) -> Result<Vec<WriterLabel>, String> {
    let mut emitted = Vec::new();
    if let Some(task) = task {
        emitted.push(WriterLabel::new(
            "task",
            task,
            "scope.task",
            "Kernel write task",
        ));
    }
    if let Some(process) = process {
        emitted.push(WriterLabel::new(
            "agentic_process",
            process,
            "scope.process",
            "Kernel write process",
        ));
    }
    if let Some(episode) = episode {
        emitted.push(WriterLabel::new(
            "agentic_episode",
            episode,
            "scope.episode",
            "Kernel write episode",
        ));
    }
    if let Some(labels) = labels {
        let object = labels.as_object().ok_or_else(|| {
            "`labels` must map each key to a non-empty array of strings".to_string()
        })?;
        let mut own = object.iter().collect::<Vec<_>>();
        own.sort_by(|left, right| left.0.cmp(right.0));
        for (key, value) in own {
            validate_label_key(key)?;
            let field = format!("labels.{key}");
            for value in label_values(value, &field)? {
                emitted.push(WriterLabel::new(key, value, &field, "Kernel write label"));
            }
        }
    }
    validate_distinct_labels(&emitted)?;
    if emitted.is_empty() {
        return Err(
            "labels must declare at least one key/value membership for temporal navigation"
                .to_string(),
        );
    }
    Ok(emitted)
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
