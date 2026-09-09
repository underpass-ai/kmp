//! Compile one about's semantic records into a single canonical ingest.
//! No member is sent to storage until all local refs and proofs are valid.

use super::validation_error::WriteValidationError;
use super::validation_errors::WriteValidationErrors;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use super::generated_ref::{generated_entry_ref, stable_idempotency_key};
use super::json_value_type::JsonValueType;
use super::plan::KernelWritePlan;
use super::planner::build_write_plan_with_local_refs;
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
    let observed_at = required_string(object, "observed_at")?;
    super::coordinates::reject_a_time_that_has_not_happened(
        &observed_at,
        crate::clock::now_seconds(),
    )
    .map_err(|error| {
        WriteValidationError::new(error)
            .at("observed_at")
            .code("FUTURE_OBSERVATION")
    })?;
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
        let kind = required_map_string(memory, "kind", &format!("memories[{index}].kind"))?;
        let summary =
            required_map_string(memory, "summary", &format!("memories[{index}].summary"))?;
        let reference = if let Some(reference) = optional_string(memory.get("ref")) {
            kmp_application::validate_supplied_entry_ref(
                &about,
                &format!("memories[{index}].ref"),
                reference,
            )
            .map_err(|error| {
                WriteValidationError::new(error)
                    .at(format!("memories[{index}].ref"))
                    .code("INVALID_REF")
            })?;
            reference.to_owned()
        } else {
            generated_entry_ref(&about, kind, summary, &identity, id)
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
    if let Some(keys) = arguments.pointer("/options/labels_new") {
        for key in keys
            .as_array()
            .ok_or("options.labels_new must be an array of label keys")?
        {
            let key = key
                .as_str()
                .ok_or("options.labels_new must contain label keys")?;
            if !object
                .get("labels")
                .is_some_and(|labels| labels.get(key).is_some())
                && !memories.iter().any(|memory| {
                    memory
                        .get("labels")
                        .is_some_and(|labels| labels.get(key).is_some())
                })
            {
                return Err(WriteValidationError::new(format!(
                    "options.labels_new names `{key}`, which no batch member declares"
                ))
                .into());
            }
        }
    }
    let mut defaults = object.clone();
    defaults.remove("memories");
    let mut errors = Vec::new();
    for (index, memory) in memories.iter().enumerate() {
        let planned = (|| {
            let memory = memory.as_object().expect("validated record");
            let id = memory["id"].as_str().expect("validated id");
            let mut request = defaults.clone();
            request.insert("idempotency_key".into(), json!(identity));
            let kind = memory["kind"].as_str().expect("validated kind");
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
            let labels =
                merged_labels(object.get("labels"), memory.get("labels")).map_err(|error| {
                    WriteValidationError::new(error).at(format!("memories[{index}].labels"))
                })?;
            request.insert("labels".into(), labels.clone());
            if let Some(options) = request.get_mut("options").and_then(Value::as_object_mut) {
                // The declaration applies only to the labels that this member uses.
                if let Some(keys) = options.get_mut("labels_new").and_then(Value::as_array_mut) {
                    keys.retain(|key| key.as_str().is_some_and(|key| labels.get(key).is_some()));
                }
                if let Some(sequence) = options.get_mut("sequence") {
                    let first = sequence
                        .as_u64()
                        .ok_or("options.sequence must be a positive integer")?;
                    let next = first
                        .checked_add(index as u64)
                        .filter(|next| *next <= u64::from(u32::MAX))
                        .ok_or("options.sequence overflows inside memories")?;
                    *sequence = json!(next);
                }
            }
            let mut links = memory
                .get("connect_to")
                .cloned()
                .unwrap_or_else(|| json!([]));
            for (link_index, link) in links
                .as_array_mut()
                .ok_or_else(|| format!("memories[{index}].connect_to must be an array"))?
                .iter_mut()
                .enumerate()
            {
                let target = link.get("ref").and_then(Value::as_str).ok_or_else(|| {
                    format!("memories[{index}].connect_to[{link_index}].ref is required")
                })?;
                if let Some(local) = target.strip_prefix('@') {
                    let reference = refs.get(local).ok_or_else(|| WriteValidationError::new(format!(
                    "memories[{index}].connect_to[{link_index}].ref names unknown local id `{local}`; declare it in memories or use an existing canonical ref"
                )).at(format!("memories[{index}].connect_to[{link_index}].ref")).code("UNKNOWN_LOCAL_REF"))?;
                    link["ref"] = json!(reference);
                }
            }
            request.insert("connect_to".into(), links);
            // A batch can record independent source facts. Strict proof validation
            // still applies to every claimed link; never fabricate links for access.
            build_write_plan_with_local_refs(&Value::Object(request), true, &targets)
                .map_err(|error| error.within(&format!("memories[{index}]")))
        })();
        match planned {
            Ok(plan) => plans.push(plan),
            Err(error) => errors.push(error),
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
    all.ingest_arguments["provenance"]["observed_at"] = object["observed_at"].clone();
    all.local_refs = refs;
    all.relation_quality_metrics = relation_quality_metrics(&all.relation_quality);
    Ok(all)
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
