//! The relabel planner: a caller's pairs, validated, compiled to the
//! kernel's relabel request. What the kernel must read to refuse — the
//! memory, its labels, the catalogue — is left to the kernel; this refuses
//! only what the arguments alone can show.

use serde_json::{Map, Value, json};

use kmp_application::{validate_ref_token, validate_supplied_entry_ref};

use super::arguments::*;
use super::coordinates::reject_a_time_that_has_not_happened;
use super::generated_ref::short_hash;
use super::relabel_plan::RelabelPlan;
use super::writer_label::validate_label_key_at;

const DEFAULT_SOURCE_KIND: &str = "agent";

pub(crate) fn build_relabel_plan(arguments: &Value) -> Result<RelabelPlan, String> {
    let arguments = arguments
        .as_object()
        .ok_or_else(|| "tool arguments must be a JSON object".to_string())?;
    let about = required_string(arguments, "about")?;
    validate_ref_token("about", &about)?;
    let reference = required_string(arguments, "ref")?;
    validate_supplied_entry_ref(&about, "ref", &reference)?;
    let actor = required_string(arguments, "actor")?;
    let observed_at = required_string(arguments, "observed_at")?;
    reject_a_time_that_has_not_happened(&observed_at, crate::clock::now_seconds())?;
    let why = required_string(arguments, "why")?;

    let add = labels_of(arguments.get("add"), "add")?;
    let remove = labels_of(arguments.get("remove"), "remove")?;
    if add.is_empty() && remove.is_empty() {
        return Err("nothing to relabel: give `add`, `remove` or both".to_string());
    }
    for (key, value) in &add {
        if remove
            .iter()
            .any(|(other_key, other)| other_key == key && other == value)
        {
            return Err(format!("`{key}={value}` is both added and removed"));
        }
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
    let intended_new = options
        .and_then(|options| options.get("labels_new"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    for key in &intended_new {
        if !add.iter().any(|(added, _)| added == key) {
            return Err(format!(
                "`options.labels_new` names `{key}`, which `add` does not carry"
            ));
        }
    }

    let idempotency_key = optional_string(arguments.get("idempotency_key"))
        .map(ToString::to_string)
        .unwrap_or_else(|| stable_relabel_key(arguments));

    let add = add
        .into_iter()
        .map(|(key, value)| json!({"key": key, "value": value}))
        .collect::<Vec<_>>();
    let remove = remove
        .into_iter()
        .map(|(key, value)| json!({"key": key, "value": value}))
        .collect::<Vec<_>>();
    let relabel_arguments = json!({
        "about": about,
        "ref": reference,
        "add": add,
        "remove": remove,
        "why": why,
        "provenance": {
            "source_kind": arguments
                .get("source_kind")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_SOURCE_KIND),
            "source_agent": actor,
            "observed_at": observed_at,
            "correlation_id": format!("kmp_relabel:{about}"),
            "causation_id": idempotency_key
        },
        "idempotency_key": idempotency_key,
        "dry_run": dry_run,
        "label_policy": if strict { "refuse" } else { "warn" },
        "intended_new": intended_new
    });

    Ok(RelabelPlan {
        about,
        reference,
        dry_run,
        add,
        remove,
        relabel_arguments,
    })
}

/// The pairs of one `add` or `remove` object, in argument order, each key
/// and value checked the way the writer checks a label.
fn labels_of(value: Option<&Value>, field: &str) -> Result<Vec<(String, String)>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let object: &Map<String, Value> = value
        .as_object()
        .ok_or_else(|| format!("`{field}` must be an object of `key: value` labels"))?;
    let mut labels = Vec::with_capacity(object.len());
    for (key, value) in object {
        validate_label_key_at(field, key)?;
        let value = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("`{field}.{key}` must be a non-empty scope id"))?;
        validate_ref_token(&format!("{field}.{key}"), value)?;
        if labels.iter().any(|(_, other)| other == value) {
            return Err(format!(
                "`{field}` uses `{value}` under two keys; within an about a scope id names one label and keeps the kind of its first use"
            ));
        }
        labels.push((key.clone(), value.to_string()));
    }
    Ok(labels)
}

/// The logical identity of a relabel: everything the caller said except
/// whether this call previews. An exact retry replays under the same key.
fn stable_relabel_key(arguments: &Map<String, Value>) -> String {
    let mut stable = arguments.clone();
    if let Some(options) = stable.get_mut("options").and_then(Value::as_object_mut) {
        options.remove("dry_run");
        if options.is_empty() {
            stable.remove("options");
        }
    }
    format!("relabel:{}", short_hash(&Value::Object(stable).to_string()))
}
