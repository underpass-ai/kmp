use std::collections::HashMap;

use kmp_proto::v1beta1::{
    AnswerPolicy, MemoryBudget, MemoryConfidence, MemoryDetailLevel, MemorySemanticClass,
    MemorySourceKind,
};
use prost_types::Timestamp;
use serde_json::{Map, Value};

pub(super) fn memory_budget_from_arguments(
    arguments: &Value,
    default_tokens: u32,
    default_depth: u32,
) -> Result<MemoryBudget, String> {
    let arguments = object(arguments, "tool arguments")?;
    let budget = optional_object_field(arguments, "budget", "budget")?;
    let tokens = budget
        .map(|budget| optional_positive_u32_field(budget, "tokens", "budget.tokens"))
        .transpose()?
        .flatten()
        .unwrap_or(default_tokens);
    let detail = budget
        .map(detail_level_from_object)
        .transpose()?
        .unwrap_or(MemoryDetailLevel::Unspecified as i32);
    let depth = match optional_positive_u32_field(arguments, "depth", "depth")? {
        Some(depth) => depth,
        None => budget
            .map(|budget| optional_positive_u32_field(budget, "depth", "budget.depth"))
            .transpose()?
            .flatten()
            .unwrap_or(default_depth),
    };
    let max_entries = budget
        .map(|budget| optional_positive_u32_field(budget, "max_entries", "budget.max_entries"))
        .transpose()?
        .flatten()
        .unwrap_or(0);

    Ok(MemoryBudget {
        tokens,
        detail,
        depth,
        max_entries,
    })
}

pub(super) fn answer_policy_from_object(arguments: &Map<String, Value>) -> Result<i32, String> {
    Ok(
        match optional_string_field(arguments, "answer_policy", "answer_policy")?.as_deref() {
            None | Some("evidence_or_unknown") => AnswerPolicy::EvidenceOrUnknown as i32,
            Some("show_conflicts") => AnswerPolicy::ShowConflicts as i32,
            Some("best_effort") => AnswerPolicy::BestEffort as i32,
            Some(other) => return Err(format!("invalid answer_policy `{other}`")),
        },
    )
}

fn detail_level_from_object(value: &Map<String, Value>) -> Result<i32, String> {
    Ok(
        match optional_string_field(value, "detail", "budget.detail")?.as_deref() {
            None => MemoryDetailLevel::Unspecified as i32,
            Some("compact") => MemoryDetailLevel::Compact as i32,
            Some("balanced") => MemoryDetailLevel::Balanced as i32,
            Some("full") => MemoryDetailLevel::Full as i32,
            Some(other) => return Err(format!("invalid budget.detail `{other}`")),
        },
    )
}

pub(super) fn object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("`{path}` must be a JSON object"))
}

pub(super) fn required_object_field<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a Map<String, Value>, String> {
    object
        .get(key)
        .ok_or_else(|| format!("missing required object argument `{path}`"))
        .and_then(|value| {
            value
                .as_object()
                .ok_or_else(|| format!("argument `{path}` must be an object"))
        })
}

pub(super) fn optional_object_field<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<&'a Map<String, Value>>, String> {
    object
        .get(key)
        .map(|value| {
            value
                .as_object()
                .ok_or_else(|| format!("argument `{path}` must be an object"))
        })
        .transpose()
}

pub(super) fn required_array_field<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a [Value], String> {
    let values = object
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing required array argument `{path}`"))?;
    if values.is_empty() {
        return Err(format!(
            "required array argument `{path}` must not be empty"
        ));
    }
    Ok(values)
}

pub(super) fn optional_array_field<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a [Value], String> {
    match object.get(key) {
        Some(value) => value
            .as_array()
            .map(Vec::as_slice)
            .ok_or_else(|| format!("argument `{path}` must be an array")),
        None => Ok(&[]),
    }
}

pub(super) fn required_string_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<String, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing required argument `{path}`"))
}

pub(super) fn optional_string_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<String>, String> {
    object
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .ok_or_else(|| format!("argument `{path}` must be a non-empty string"))
        })
        .transpose()
}

pub(super) fn optional_string_array_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Vec<String>, String> {
    let Some(value) = object.get(key) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("argument `{path}` must be an array"))?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .ok_or_else(|| format!("argument `{path}[{index}]` must be a non-empty string"))
        })
        .collect()
}

pub(super) fn optional_metadata_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<HashMap<String, String>, String> {
    let Some(metadata) = optional_object_field(object, key, path)? else {
        return Ok(HashMap::new());
    };
    metadata
        .iter()
        .map(|(key, value)| {
            let value = value
                .as_str()
                .ok_or_else(|| format!("argument `{path}.{key}` must be a string"))?;
            Ok((key.clone(), value.to_string()))
        })
        .collect()
}

pub(super) fn required_timestamp_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Timestamp, String> {
    let value = required_string_field(object, key, path)?;
    parse_timestamp(&value, path)
}

pub(super) fn optional_timestamp_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<Timestamp>, String> {
    optional_string_field(object, key, path)?
        .map(|value| parse_timestamp(&value, path))
        .transpose()
}

fn parse_timestamp(value: &str, path: &str) -> Result<Timestamp, String> {
    value
        .parse::<Timestamp>()
        .map_err(|error| format!("argument `{path}` must be an RFC3339 timestamp: {error}"))
}

pub(super) fn optional_bool_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<bool>, String> {
    object
        .get(key)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| format!("argument `{path}` must be a boolean"))
        })
        .transpose()
}

pub(super) fn optional_u32_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<u32>, String> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    let value = value
        .as_u64()
        .ok_or_else(|| format!("argument `{path}` must be an integer"))?;
    u32::try_from(value)
        .map(Some)
        .map_err(|_| format!("argument `{path}` must fit in uint32"))
}

pub(super) fn optional_positive_u32_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<u32>, String> {
    let value = optional_u32_field(object, key, path)?;
    if value == Some(0) {
        return Err(format!("argument `{path}` must be greater than zero"));
    }
    Ok(value)
}

pub(super) fn semantic_class_from_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<i32, String> {
    Ok(match required_string_field(object, key, path)?.as_str() {
        "structural" => MemorySemanticClass::Structural as i32,
        "causal" => MemorySemanticClass::Causal as i32,
        "motivational" => MemorySemanticClass::Motivational as i32,
        "procedural" => MemorySemanticClass::Procedural as i32,
        "evidential" => MemorySemanticClass::Evidential as i32,
        "constraint" => MemorySemanticClass::Constraint as i32,
        other => return Err(format!("invalid memory relation class `{other}`")),
    })
}

pub(super) fn confidence_from_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<i32, String> {
    Ok(match optional_string_field(object, key, path)?.as_deref() {
        None => MemoryConfidence::Unspecified as i32,
        Some("high") => MemoryConfidence::High as i32,
        Some("medium") => MemoryConfidence::Medium as i32,
        Some("low") => MemoryConfidence::Low as i32,
        Some("unknown") => MemoryConfidence::Unknown as i32,
        Some(other) => return Err(format!("invalid memory relation confidence `{other}`")),
    })
}

pub(super) fn source_kind_from_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<i32, String> {
    Ok(match required_string_field(object, key, path)?.as_str() {
        "human" => MemorySourceKind::Human as i32,
        "agent" => MemorySourceKind::Agent as i32,
        "projection" => MemorySourceKind::Projection as i32,
        "derived" => MemorySourceKind::Derived as i32,
        other => return Err(format!("invalid memory provenance source_kind `{other}`")),
    })
}
