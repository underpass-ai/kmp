use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::ingest_arguments::*;

use kmp_application::validate_supplied_member_ref;

pub(super) fn validate_provenance(
    provenance: &Map<String, Value>,
    allow_missing_observation: bool,
) -> Result<(), String> {
    validate_source_kind(required_map_string(
        provenance,
        "source_kind",
        "provenance.source_kind",
    )?)?;
    let _source_agent = required_map_string(provenance, "source_agent", "provenance.source_agent")?;
    if !allow_missing_observation
        || provenance
            .get("observed_at")
            .is_some_and(|value| !value.is_null())
    {
        required_map_string(provenance, "observed_at", "provenance.observed_at")?;
    }
    Ok(())
}

pub(super) fn validate_source_kind(value: &str) -> Result<(), String> {
    match value.trim() {
        "human" | "agent" | "projection" | "derived" => Ok(()),
        other => Err(format!("invalid memory provenance source_kind `{other}`")),
    }
}

pub(super) fn validate_semantic_class(value: &str) -> Result<(), String> {
    match value.trim() {
        "structural" | "causal" | "motivational" | "procedural" | "evidential" | "constraint" => {
            Ok(())
        }
        other => Err(format!("invalid memory relation class `{other}`")),
    }
}

pub(super) fn validate_confidence(value: &str) -> Result<(), String> {
    match value.trim() {
        "high" | "medium" | "low" | "unknown" => Ok(()),
        other => Err(format!("invalid memory relation confidence `{other}`")),
    }
}

pub(super) fn validate_relation_explanation(
    relation: &Value,
    semantic_class: &str,
) -> Result<(), String> {
    let confidence = relation
        .get("confidence")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    if let Some(confidence) = confidence {
        validate_confidence(confidence)?;
    }
    if semantic_class != "structural" {
        if confidence.is_none() {
            return Err("non-structural memory relations require confidence".to_string());
        }
        let has_why = relation
            .get("why")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
        let has_evidence = relation
            .get("evidence")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
        if !has_why && !has_evidence {
            return Err("non-structural memory relations require why or evidence".to_string());
        }
    }
    Ok(())
}

pub(super) fn validate_ingest_member_ref(
    about: &str,
    path: &str,
    reference: &str,
    dimension_kinds: &BTreeMap<&str, BTreeSet<&str>>,
) -> Result<(), String> {
    if let Some(kinds) = dimension_kinds.get(reference) {
        if kinds.len() > 1 {
            return Err(format!(
                "`{path}` dimension `{reference}` is ambiguous across keys; use the exact label ref"
            ));
        }
        Ok(())
    } else {
        validate_supplied_member_ref(about, path, reference)
    }
}

pub(super) fn validate_evidence_supports(
    about: &str,
    evidence_item: &Value,
    dimension_kinds: &BTreeMap<&str, BTreeSet<&str>>,
) -> Result<(), String> {
    for (index, support) in evidence_item
        .get("supports")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let Some(support) = support
            .as_str()
            .filter(|support| !support.trim().is_empty())
        else {
            return Err(format!(
                "argument `memory.evidence[].supports[{index}]` must be a non-empty string"
            ));
        };
        validate_ingest_member_ref(
            about,
            &format!("memory.evidence[].supports[{index}]"),
            support,
            dimension_kinds,
        )?;
    }
    Ok(())
}

pub(super) fn entry_scopes(entry: &Value) -> Vec<String> {
    entry
        .get("coordinates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|coordinate| coordinate.get("scope_id"))
        .filter_map(Value::as_str)
        .filter(|scope| !scope.trim().is_empty())
        .map(ToString::to_string)
        .collect()
}

pub(super) fn validate_entry_positions(
    entry: &Value,
    dimension_kinds: &BTreeMap<&str, BTreeSet<&str>>,
) -> Result<(), String> {
    let positions = entry
        .get("coordinates")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "missing required array argument `memory.entries[].coordinates`".to_string()
        })?;
    if positions.is_empty() {
        return Err(
            "required array argument `memory.entries[].coordinates` must not be empty".to_string(),
        );
    }

    for position in positions {
        validate_coordinate(position, dimension_kinds, "memory.entries[].coordinates[]")?;
    }

    Ok(())
}

pub(super) fn validate_coordinate(
    coordinate: &Value,
    dimension_kinds: &BTreeMap<&str, BTreeSet<&str>>,
    path: &str,
) -> Result<(), String> {
    let Some(dimension) = coordinate.get("dimension").and_then(Value::as_str) else {
        return Err(format!("{path} is missing required `dimension`"));
    };
    if dimension.trim().is_empty() {
        return Err(format!("{path}.dimension must not be empty"));
    }
    let Some(scope_id) = coordinate.get("scope_id").and_then(Value::as_str) else {
        return Err(format!("{path} is missing required `scope_id`"));
    };
    if scope_id.trim().is_empty() {
        return Err(format!("{path}.scope_id must not be empty"));
    }
    let Some(expected_kind) = dimension_kinds.get(scope_id) else {
        return Err(format!(
            "{path} references unknown dimension scope `{scope_id}`"
        ));
    };
    if !expected_kind.contains(dimension) {
        let expected_kind = expected_kind.iter().copied().collect::<Vec<_>>().join(", ");
        return Err(format!(
            "{path}.dimension `{dimension}` does not match declared kind `{expected_kind}` for scope `{scope_id}`"
        ));
    }
    Ok(())
}

pub(super) fn evidence_scopes(evidence_item: &Value) -> Vec<String> {
    evidence_item
        .get("supports")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|scope| !scope.trim().is_empty())
        .map(ToString::to_string)
        .collect()
}

pub(super) fn stable_payload_json(value: &Value) -> Result<String, String> {
    serde_json::to_string(value)
        .map_err(|error| format!("failed to encode ingest payload: {error}"))
}

pub(super) fn memory_id_from_idempotency_key(idempotency_key: &str) -> String {
    idempotency_key
        .strip_prefix("ingest:")
        .map(|suffix| format!("memory:{suffix}"))
        .unwrap_or_else(|| format!("memory:{idempotency_key}"))
}
