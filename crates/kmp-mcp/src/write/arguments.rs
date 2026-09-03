use serde_json::{Map, Value};

pub(super) fn required_object<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Map<String, Value>, String> {
    object
        .get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("missing required object argument `{key}`"))
}

/// Refuses a stamp the kernel's own clock says has not happened yet.
///
/// The read path is ordered by `observed_at`, so an entry above the present
/// is one `kmp_forward` from a correct "now" will never return: the delta
/// comes back empty and looks exactly like a quiet week. The log has no
/// delete, so this has to be caught before the write and not explained
/// afterwards.
///
/// A shape the clock cannot read is left alone. Format is the ingest layer's
/// contract, and refusing here for a second reason would move that argument
/// into the wrong place.
pub(super) fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing required argument `{key}`"))
}

pub(super) fn required_map_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a str, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing required argument `{path}`"))
}

pub(super) fn optional_map_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Option<&'a str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

pub(super) fn optional_string(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

pub(super) fn optional_array<'a>(
    value: Option<&'a Value>,
    key: &str,
) -> Result<&'a [Value], String> {
    match value {
        Some(value) => value
            .as_array()
            .map(Vec::as_slice)
            .ok_or_else(|| format!("argument `{key}` must be an array")),
        None => Ok(&[]),
    }
}

pub(super) fn required_relation_string<'a>(
    relation: &'a Map<String, Value>,
    key: &str,
    semantic_class: &str,
    index: usize,
) -> Result<&'a str, String> {
    if semantic_class == "structural" {
        return Ok(optional_map_string(relation, key).unwrap_or(""));
    }
    required_map_string(relation, key, &format!("connect_to[{index}].{key}"))
}

pub(super) fn reject_duplicate_ref(refs: &mut Vec<String>, new_ref: &str) -> Result<(), String> {
    if refs.iter().any(|existing| existing == new_ref) {
        return Err(format!("generated duplicate memory ref `{new_ref}`"));
    }
    refs.push(new_ref.to_string());
    Ok(())
}

pub(super) fn validate_intent(value: &str) -> Result<(), String> {
    match value {
        "record_turn" | "record_observation" | "record_decision" | "record_feedback"
        | "record_delta" | "record_summary" => Ok(()),
        other => Err(format!("invalid kmp_write_memory intent `{other}`")),
    }
}

pub(super) fn validate_node_kind(value: &str) -> Result<(), String> {
    match value {
        "turn" | "observation" | "decision" | "feedback" | "semantic_delta" | "constraint"
        | "preference" | "derived_value" | "error_path" | "success_path" => Ok(()),
        other => Err(format!("invalid kmp_write_memory current.kind `{other}`")),
    }
}

pub(super) fn validate_semantic_class(value: &str) -> Result<(), String> {
    match value {
        "structural" | "causal" | "motivational" | "procedural" | "evidential" | "constraint" => {
            Ok(())
        }
        other => Err(format!("invalid kmp_write_memory relation class `{other}`")),
    }
}

pub(super) fn validate_confidence(value: &str) -> Result<(), String> {
    match value {
        "high" | "medium" | "low" | "unknown" => Ok(()),
        other => Err(format!(
            "invalid kmp_write_memory relation confidence `{other}`"
        )),
    }
}
