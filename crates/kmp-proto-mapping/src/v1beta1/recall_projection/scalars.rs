//! The scalar edge of the recall mapping: enum labels, timestamps and the
//! pointer reads that turn projected JSON back into typed fields.

use kmp_proto::v1beta1::{MemoryConfidence, MemoryDetailLevel, MemorySemanticClass};
use prost_types::Timestamp;
use serde_json::{Map, Value, json};

pub(super) fn insert_non_empty(object: &mut Map<String, Value>, key: &str, value: &str) {
    if !value.trim().is_empty() {
        object.insert(key.to_string(), json!(value));
    }
}

pub(super) fn insert_timestamp(
    object: &mut Map<String, Value>,
    key: &str,
    value: Option<Timestamp>,
) {
    if let Some(value) = value {
        object.insert(key.to_string(), json!(value.to_string()));
    }
}

pub(super) fn detail_label(value: i32) -> &'static str {
    match MemoryDetailLevel::try_from(value) {
        Ok(MemoryDetailLevel::Compact) => "compact",
        Ok(MemoryDetailLevel::Full) => "full",
        _ => "balanced",
    }
}

pub(super) fn detail_from_label(value: &str) -> i32 {
    match value {
        "compact" => MemoryDetailLevel::Compact as i32,
        "full" => MemoryDetailLevel::Full as i32,
        _ => MemoryDetailLevel::Balanced as i32,
    }
}

pub(super) fn semantic_class_label(value: i32) -> &'static str {
    match MemorySemanticClass::try_from(value) {
        Ok(MemorySemanticClass::Structural) => "structural",
        Ok(MemorySemanticClass::Causal) => "causal",
        Ok(MemorySemanticClass::Motivational) => "motivational",
        Ok(MemorySemanticClass::Procedural) => "procedural",
        Ok(MemorySemanticClass::Evidential) => "evidential",
        Ok(MemorySemanticClass::Constraint) => "constraint",
        _ => "unspecified",
    }
}

pub(super) fn confidence_label(value: i32) -> &'static str {
    match MemoryConfidence::try_from(value) {
        Ok(MemoryConfidence::High) => "high",
        Ok(MemoryConfidence::Medium) => "medium",
        Ok(MemoryConfidence::Low) => "low",
        Ok(MemoryConfidence::Unknown) => "unknown",
        _ => "unspecified",
    }
}

pub(super) fn confidence_from_label(value: &str) -> i32 {
    match value {
        "high" => MemoryConfidence::High as i32,
        "medium" => MemoryConfidence::Medium as i32,
        "low" => MemoryConfidence::Low as i32,
        _ => MemoryConfidence::Unknown as i32,
    }
}

pub(super) fn string_at(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(super) fn strings_at(value: &Value, pointer: &str) -> Vec<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect()
}

pub(super) fn u64_at(value: &Value, pointer: &str) -> u64 {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

pub(super) fn u32_at(value: &Value, pointer: &str) -> u32 {
    u64_at(value, pointer).try_into().unwrap_or(u32::MAX)
}
