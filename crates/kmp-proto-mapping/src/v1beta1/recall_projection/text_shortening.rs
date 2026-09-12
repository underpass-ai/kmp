//! Shortening recall prose, and only prose: how far a core's text can be
//! bounded and which keys are data that must survive intact.

use serde_json::Value;

use super::json_paths::is_reference_key;

pub(super) fn max_text_chars(value: &Value) -> usize {
    match value {
        Value::String(text) => text.chars().count(),
        Value::Array(items) => items.iter().map(max_text_chars).max().unwrap_or(0),
        Value::Object(object) => object
            .iter()
            .filter(|(key, value)| can_shorten(key, value))
            .map(|(_, value)| max_text_chars(value))
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}

pub(super) fn truncate_json_text(value: &mut Value, max_chars: usize) -> usize {
    // A proof item's provenance is data, not prose. Keep its source, clock,
    // metadata and supports unchanged so typed projection can preserve it.
    if value.get("id").is_some()
        && value.get("supports").is_some()
        && let Some(text) = value.get_mut("text")
    {
        return truncate_json_text(text, max_chars);
    }
    match value {
        Value::String(text) => {
            let total = text.chars().count();
            if total <= max_chars {
                0
            } else {
                let mut bounded = text.chars().take(max_chars).collect::<String>();
                bounded.push('…');
                *text = bounded;
                total - max_chars
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .map(|item| truncate_json_text(item, max_chars))
            .sum(),
        Value::Object(object) => object
            .iter_mut()
            .filter(|(key, value)| can_shorten(key, value))
            .map(|(_, value)| truncate_json_text(value, max_chars))
            .sum(),
        _ => 0,
    }
}

// Only prose can be shortened. Enums, clocks, matched terms, identifiers
// and provenance remain data even when their serialized representation is text.
fn can_shorten(key: &str, value: &Value) -> bool {
    if is_reference_key(key) || key == "metadata" {
        return false;
    }
    matches!(
        key,
        "summary"
            | "answer"
            | "text"
            | "why"
            | "evidence"
            | "because"
            | "objective"
            | "current_state"
            | "open_loops"
            | "next_actions"
            | "guardrails"
    ) || value.is_object()
        || value
            .as_array()
            .is_some_and(|items| items.iter().any(Value::is_object))
}
