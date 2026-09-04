use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::ingest_arguments::*;

use kmp_application::{validate_ref_token, validate_supplied_member_ref};

pub(super) fn validate_provenance(provenance: &Map<String, Value>) -> Result<(), String> {
    validate_source_kind(required_map_string(
        provenance,
        "source_kind",
        "provenance.source_kind",
    )?)?;
    let _source_agent = required_map_string(provenance, "source_agent", "provenance.source_agent")?;
    let _observed_at = required_map_string(provenance, "observed_at", "provenance.observed_at")?;
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
    dimension_kinds: &BTreeMap<&str, &str>,
) -> Result<(), String> {
    if dimension_kinds.contains_key(reference) {
        validate_ref_token(path, reference)
    } else {
        validate_supplied_member_ref(about, path, reference)
    }
}

pub(super) fn validate_evidence_supports(
    about: &str,
    evidence_item: &Value,
    dimension_kinds: &BTreeMap<&str, &str>,
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
    dimension_kinds: &BTreeMap<&str, &str>,
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
    dimension_kinds: &BTreeMap<&str, &str>,
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
    if dimension != *expected_kind {
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

/// The about boundary of a raw `kmp_ingest`, and nothing else: a relation
/// endpoint or an evidence support that names another about is refused
/// here, before any backend sees it. The kernel admits one relation across
/// abouts — an equivalence a writer declared from a `kmp_relate` proposal —
/// and only `kmp_write_memory` may declare it. Everything else about the
/// payload, dimension aliases included, is the kernel's to judge.
pub(crate) fn reject_refs_outside_about(arguments: &Value) -> Result<(), String> {
    let arguments = arguments
        .as_object()
        .ok_or_else(|| "tool arguments must be a JSON object".to_string())?;
    let about = arguments
        .get("about")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|about| !about.is_empty())
        .ok_or_else(|| "missing required argument `about`".to_string())?;
    let Some(memory) = arguments.get("memory").and_then(Value::as_object) else {
        return Ok(());
    };
    let items = |key: &str| {
        memory
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let check = |path: &str, reference: &str| -> Result<(), String> {
        if names_another_about(about, reference) {
            return Err(format!(
                "`{path}` `{reference}` does not belong to about `{about}`; raw kmp_ingest never crosses an about — a writer declares an equivalence with kmp_write_memory"
            ));
        }
        Ok(())
    };
    for relation in items("relations") {
        for (key, path) in [
            ("from", "memory.relations[].from"),
            ("to", "memory.relations[].to"),
        ] {
            if let Some(reference) = relation.get(key).and_then(Value::as_str) {
                check(path, reference)?;
            }
        }
    }
    for evidence in items("evidence") {
        for supported in evidence
            .get("supports")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            check("memory.evidence[].supports[]", supported)?;
        }
    }
    Ok(())
}

/// Whether a ref names a node of another about: three or more segments
/// whose leading two are not this about. A bare dimension id has fewer,
/// and this about's own refs and namespaced dimensions start with it.
fn names_another_about(about: &str, reference: &str) -> bool {
    let reference = reference.trim();
    if reference == about
        || reference.starts_with(&format!("{about}:"))
        || reference.starts_with(&format!("evidence:{about}:"))
        || reference.starts_with(&format!("about:{about}:dimension:"))
    {
        return false;
    }
    reference.split(':').count() >= 3
}

#[cfg(test)]
mod boundary_tests {
    use super::reject_refs_outside_about;
    use serde_json::json;

    #[test]
    fn a_raw_ingest_may_name_its_own_refs_and_dimensions_but_never_another_about() {
        let own = json!({"about": "service:alpha", "memory": {
            "dimensions": [],
            "entries": [{"id": "claim:short", "kind": "claim", "text": "t"}],
            "relations": [{"from": "conversation:rachel", "to": "service:alpha:claim:x", "rel": "contains_entry", "class": "structural"}],
            "evidence": [{"id": "evidence:service:alpha:e", "supports": ["service:alpha:claim:x", "about:service:alpha:dimension:work:main"], "text": "t"}]
        }});
        assert!(reject_refs_outside_about(&own).is_ok());

        let foreign = json!({"about": "service:alpha", "memory": {
            "relations": [{"from": "service:alpha:claim:x", "to": "service:beta:outcome:freeze", "rel": "same_event_as", "class": "evidential", "method": "kmp_relate:identifier"}]
        }});
        let error = reject_refs_outside_about(&foreign).expect_err("another about");
        assert!(
            error.contains("does not belong to about `service:alpha`"),
            "{error}"
        );
        assert!(error.contains("kmp_write_memory"), "{error}");
    }
}
