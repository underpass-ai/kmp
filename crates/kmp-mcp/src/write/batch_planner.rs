//! Compile one about's semantic records into a single canonical ingest.
//! No member is sent to storage until all local refs and proofs are valid.

use super::validation_error::WriteValidationError;
use super::validation_errors::WriteValidationErrors;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use super::batch_member::Member;
use super::generated_ref::{generated_entry_ref, stable_idempotency_key};
use super::json_value_type::JsonValueType;
use super::plan::KernelWritePlan;
use super::planner::build_write_plan_with_local_refs;
use super::proof_observation::preserve_proof_observation;
use super::relation_quality::relation_quality_metrics;
use super::validated_arguments::{optional_string, required_map_string, required_string};

pub(crate) fn build_batch_plan(
    arguments: &Value,
) -> Result<KernelWritePlan, WriteValidationErrors> {
    let object = arguments
        .as_object()
        .ok_or("tool arguments must be an object")?;
    let about = required_string(object, "about")?;
    kmp_application::validate_ref_token("about", &about)?;
    required_string(object, "actor")?;
    super::coordinates::observation_time(object)?;
    super::review_token::validate_review_token(object.get("review_token"))?;
    for field in ["current", "intent", "semantic_delta", "connect_to", "scope"] {
        if object.contains_key(field) {
            return Err(WriteValidationError::new(format!(
                "`{field}` cannot accompany memories; declare each record's kind, labels and connect_to inside memories"
            )).into());
        }
    }
    let memories = object.get("memories").ok_or_else(|| {
        WriteValidationError::new("memories is required")
            .at("memories")
            .code("REQUIRED_FIELD")
    })?;
    let memories = memories.as_array().ok_or_else(|| {
        WriteValidationError::wrong_type("memories", JsonValueType::Array, memories)
    })?;
    if memories.is_empty() {
        return Err(
            WriteValidationError::new("memories must contain at least one record")
                .at("memories")
                .code("EMPTY_MEMORIES")
                .into(),
        );
    }
    let identity = optional_string(object.get("idempotency_key"))
        .map(str::to_owned)
        .unwrap_or_else(|| stable_idempotency_key(object));
    let mut refs = BTreeMap::new();
    let mut targets = BTreeSet::new();
    // A caller-chosen ref updates the entry it names. The result has to say
    // so, because the same packet shape also creates memories (#663).
    let mut supplied = BTreeSet::new();
    // What a record got wrong that does not stop the packet being read, and
    // the compiler's own finding each of those stands in for.
    let mut early = Vec::new();
    let mut superseded = BTreeMap::new();
    // Resolve forward references as well as references to earlier records.
    for (index, memory) in memories.iter().enumerate() {
        let memory = memory.as_object().ok_or_else(|| {
            WriteValidationError::wrong_type(
                &format!("memories[{index}]"),
                JsonValueType::Object,
                memory,
            )
        })?;
        let id = required_map_string(memory, "id", &format!("memories[{index}].id"))?;
        if !id.as_bytes()[0].is_ascii_alphabetic()
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return Err(WriteValidationError::new(format!(
                "memories[{index}].id must start with a letter and contain only letters, digits, _ or -"
            )).at(format!("memories[{index}].id")).code("INVALID_LOCAL_ID").into());
        }
        // A missing kind or summary is the compiler's to report, with the
        // rest of the record; the generated ref only needs to be distinct.
        // A placeholder kind keeps that ref well formed; the record fails anyway.
        let kind = optional_string(memory.get("kind")).unwrap_or("memory");
        let summary = optional_string(memory.get("summary")).unwrap_or_default();
        if let Err(error) = super::expansion_selection::ExpansionSelection::proposed(
            memory.get("search_expansions"),
            &format!("memories[{index}].search_expansions"),
        ) {
            early.push(error);
        }
        let supplied_ref = optional_string(memory.get("ref")).filter(|reference| {
            match kmp_application::validate_supplied_entry_ref(
                &about,
                &format!("memories[{index}].ref"),
                reference,
            ) {
                Ok(()) => true,
                Err(error) => {
                    early.push(
                        WriteValidationError::new(error)
                            .at(format!("memories[{index}].ref"))
                            .code("INVALID_REF"),
                    );
                    superseded.insert(index, "ref");
                    false
                }
            }
        });
        let reference = match supplied_ref {
            Some(reference) => {
                supplied.insert(reference.to_owned());
                reference.to_owned()
            }
            None => generated_entry_ref(&about, kind, summary, &identity, id),
        };
        if refs.insert(id.to_owned(), reference.clone()).is_some() {
            return Err(WriteValidationError::new(format!(
                "memories[{index}].id repeats local id `{id}`"
            ))
            .at(format!("memories[{index}].id"))
            .code("DUPLICATE_LOCAL_ID")
            .into());
        }
        if !targets.insert(reference.clone()) {
            return Err(WriteValidationError::new(format!(
                "memories[{index}].ref repeats write target `{reference}`"
            ))
            .at(format!("memories[{index}].ref"))
            .code("DUPLICATE_TARGET")
            .into());
        }
    }

    let mut plans = Vec::new();
    // `options.labels_new` is judged once for the packet, with the records:
    // a malformed declaration is one failure at its own field, and the
    // records are compiled without it rather than each refusing it again.
    let mut labels_new_unusable = false;
    if let Some(keys) = arguments.pointer("/options/labels_new") {
        let shape = || {
            WriteValidationError::new("options.labels_new must be an array of label keys")
                .at("options.labels_new")
                .code("INVALID_LABELS_NEW")
                .global()
        };
        match keys.as_array() {
            None => {
                early.push(shape());
                labels_new_unusable = true;
            }
            Some(keys) => {
                for key in keys {
                    let Some(key) = key.as_str() else {
                        early.push(shape());
                        labels_new_unusable = true;
                        break;
                    };
                    if !object
                        .get("labels")
                        .is_some_and(|labels| labels.get(key).is_some())
                        && !memories.iter().any(|memory| {
                            memory
                                .get("labels")
                                .is_some_and(|labels| labels.get(key).is_some())
                        })
                    {
                        early.push(
                            WriteValidationError::new(format!(
                                "options.labels_new names `{key}`, which no batch member declares"
                            ))
                            .at("options.labels_new")
                            .code("INVALID_LABELS_NEW")
                            .global(),
                        );
                    }
                }
            }
        }
    }
    let mut defaults = object.clone();
    defaults.remove("memories");
    if labels_new_unusable
        && let Some(options) = defaults.get_mut("options").and_then(Value::as_object_mut)
    {
        options.remove("labels_new");
    }
    let mut errors = early;
    for (index, memory) in memories.iter().enumerate() {
        let at = format!("memories[{index}]");
        let member = Member {
            index,
            memory,
            superseded: superseded.get(&index).copied(),
        };
        match plan_member(member, &defaults, object, &identity, &refs, &targets) {
            Ok(mut plan) => {
                preserve_proof_observation(&mut plan.ingest_arguments);
                plans.push(plan);
            }
            Err(failures) => errors.extend(failures.into_iter().map(|error| error.within(&at))),
        }
    }
    if let Some(errors) = WriteValidationErrors::collected(errors) {
        return Err(errors);
    }
    let mut all = plans.remove(0);
    for plan in plans {
        for field in ["dimensions", "entries", "relations", "evidence"] {
            let destination = all.ingest_arguments["memory"][field]
                .as_array_mut()
                .expect("compiled array");
            for item in plan.ingest_arguments["memory"][field]
                .as_array()
                .expect("compiled array")
            {
                if field == "dimensions"
                    && destination.iter().any(|prior| prior["id"] == item["id"])
                {
                    continue;
                }
                destination.push(item.clone());
            }
        }
        all.generated_refs.extend(plan.generated_refs);
        for label in plan.labels {
            if !all.labels.contains(&label) {
                all.labels.push(label);
            }
        }
        all.relations.extend(plan.relations);
        all.relation_quality.extend(plan.relation_quality);
        all.diagnostics.extend(plan.diagnostics);
        all.next_suggested_reads.extend(plan.next_suggested_reads);
    }
    // Provenance carries observation of the whole packet; each entry retains
    // its own observed/occurred/valid clocks. Ingestion is assigned by the kernel.
    all.ingest_arguments["provenance"]["observed_at"] =
        object.get("observed_at").cloned().unwrap_or(Value::Null);
    super::coordinates::omit_unknown_observations(&mut all.ingest_arguments);
    all.local_refs = refs;
    all.relation_quality_metrics = relation_quality_metrics(&all.relation_quality);
    all.replaced = super::replacement_view::replaced_memories(&all.ingest_arguments, &supplied);
    Ok(all)
}

/// One record of the packet compiled as its own write, every failure of it
/// reported together. A failure found here, before the compiler runs, stands
/// in for what the compiler would derive from the same cause: a record whose
/// labels cannot be read is not also told it has none, and a link to an
/// unknown local id is not also judged as a link.
fn plan_member(
    member: Member<'_>,
    defaults: &Map<String, Value>,
    packet: &Map<String, Value>,
    identity: &str,
    refs: &BTreeMap<String, String>,
    targets: &BTreeSet<String>,
) -> Result<KernelWritePlan, Vec<WriteValidationError>> {
    let index = member.index;
    let memory = member.memory.as_object().expect("validated record");
    let id = memory["id"].as_str().expect("validated id");
    let mut failures = Vec::new();
    let mut superseded = member
        .superseded
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut request = defaults.clone();
    request.insert("idempotency_key".into(), json!(identity));
    let kind = memory
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let intent = match kind {
        "turn" => "record_turn",
        "decision" => "record_decision",
        "feedback" => "record_feedback",
        _ => "record_observation",
    };
    request.insert("intent".into(), json!(intent));
    let mut current = Map::new();
    for field in ["kind", "summary", "summary_en", "evidence"] {
        if let Some(value) = memory.get(field) {
            current.insert(field.into(), value.clone());
        }
    }
    current.insert("ref".into(), json!(refs[id]));
    request.insert("current".into(), Value::Object(current));
    for field in [
        "observed_at",
        "occurred_at",
        "valid_from",
        "valid_until",
        "rank",
    ] {
        if let Some(value) = memory.get(field) {
            request.insert(field.into(), value.clone());
        }
    }
    let labels = match merged_labels(packet.get("labels"), memory.get("labels")) {
        Ok(labels) => labels,
        Err(error) => {
            failures.push(
                WriteValidationError::new(error)
                    .at("labels")
                    .code("INVALID_LABELS"),
            );
            superseded.push("labels".to_owned());
            json!({})
        }
    };
    request.insert("labels".into(), labels.clone());
    if let Some(options) = request.get_mut("options").and_then(Value::as_object_mut) {
        // The declaration applies only to the labels that this member uses.
        if let Some(keys) = options.get_mut("labels_new").and_then(Value::as_array_mut) {
            keys.retain(|key| key.as_str().is_some_and(|key| labels.get(key).is_some()));
        }
        if let Some(sequence) = options.get_mut("sequence") {
            let next = sequence
                .as_u64()
                .ok_or("options.sequence must be a positive integer")
                .and_then(|first| {
                    first
                        .checked_add(index as u64)
                        .filter(|next| *next <= u64::from(u32::MAX))
                        .ok_or("options.sequence overflows inside memories")
                });
            match next {
                Ok(next) => *sequence = json!(next),
                Err(error) => {
                    // A packet-wide option: said once, without a record path.
                    return Err(vec![
                        WriteValidationError::new(error)
                            .at("options.sequence")
                            .global(),
                    ]);
                }
            }
        }
    }
    let mut links = memory
        .get("connect_to")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let Some(declared) = links.as_array_mut() else {
        return Err(vec![WriteValidationError::wrong_type(
            "connect_to",
            JsonValueType::Array,
            &links,
        )]);
    };
    let mut resolved = Vec::new();
    for (link_index, link) in declared.iter_mut().enumerate() {
        let Some(target) = link.get("ref").and_then(Value::as_str) else {
            // The compiler names the missing ref itself.
            resolved.push(link.clone());
            continue;
        };
        let local = target.strip_prefix('@').unwrap_or(target);
        if let Some(reference) = refs.get(local) {
            link["ref"] = json!(reference);
        } else if target.starts_with('@') || !target.contains(':') {
            failures.push(
                WriteValidationError::new(format!(
                    "unknown local id `{local}`; use an exact id declared in memories or a canonical ref returned by a read"
                ))
                .at(format!("connect_to[{link_index}].ref"))
                .code("UNKNOWN_LOCAL_REF")
                .allowed_values(refs.keys()),
            );
            superseded.push(format!("connect_to[{link_index}]"));
        }
        resolved.push(link.clone());
    }
    request.insert("connect_to".into(), Value::Array(resolved));
    // A batch can record independent source facts. Strict proof validation
    // still applies to every claimed link; never fabricate links for access.
    match build_write_plan_with_local_refs(&Value::Object(request), true, targets) {
        Ok(plan) if failures.is_empty() => Ok(plan),
        Ok(_) => Err(failures),
        Err(compiled) => {
            failures.extend(compiled.into_iter().filter(|failure| {
                !superseded
                    .iter()
                    .any(|cause| failure.field_is_within(cause))
            }));
            Err(failures)
        }
    }
}

fn merged_labels(common: Option<&Value>, own: Option<&Value>) -> Result<Value, String> {
    let mut labels = Map::new();
    for source in [common, own].into_iter().flatten() {
        let source = source.as_object().ok_or("labels must map keys to arrays")?;
        for (key, values) in source {
            let values = values
                .as_array()
                .filter(|values| !values.is_empty())
                .ok_or_else(|| format!("{key} must be a non-empty array of strings"))?;
            let mut seen = BTreeSet::new();
            for value in values {
                let value = value
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| format!("{key} must contain non-empty strings"))?;
                if !seen.insert(value) {
                    return Err(format!("{key} repeats value `{value}`"));
                }
                let result = labels
                    .entry(key.clone())
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .expect("label array");
                if !result.iter().any(|prior| prior == value) {
                    result.push(json!(value));
                }
            }
        }
    }
    Ok(Value::Object(labels))
}
