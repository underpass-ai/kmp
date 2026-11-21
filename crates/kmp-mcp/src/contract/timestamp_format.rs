//! `"format": "date-time"` on every timestamp-valued string of the input
//! schemas.
//!
//! A clock argument must be an RFC 3339 timestamp with its zone (`Z` or an
//! offset; `clock::require_rfc3339`), and a bare date is refused. Since the
//! catalogue diet the published schema said nothing about it, and agents sent
//! `"2026-08-01"`. The standard JSON-Schema hint says it in five tokens. It is
//! an annotation: the law (`schema_law`) neither accepts nor refuses anything
//! because of it. `tests/timestamp_format.rs` holds the tagged set to what the
//! validator actually checks as a timestamp, both ways.

use serde_json::Value;

/// Property names that hold a timestamp wherever they appear.
const CLOCK_KEYS: [&str; 8] = [
    "occurred_at",
    "observed_at",
    "ingested_at",
    "valid_from",
    "valid_until",
    // `as_of.time` and the `from` / `at` / `around` cursors of kmp_time,
    // `memory.evidence[].time` of kmp_ingest.
    "time",
    // `interval.start` and `interval.end`.
    "start",
    "end",
];

/// `from` and `to` hold a timestamp only where they bound a span on the
/// clock; elsewhere they are refs.
fn span_bound(tool: &str, parent: &str, key: &str) -> bool {
    matches!(key, "from" | "to")
        && (parent == "time_range" || (tool == "kmp_view_read_projection" && parent.is_empty()))
}

/// The tools' input schemas with every timestamp string tagged.
pub(crate) fn with_timestamp_formats(mut result: Value) -> Value {
    for tool in result["tools"].as_array_mut().into_iter().flatten() {
        let name = tool["name"].as_str().unwrap_or_default().to_owned();
        if let Some(schema) = tool.get_mut("inputSchema") {
            tag(&name, "", schema);
        }
    }
    result
}

fn is_string(schema: &Value) -> bool {
    match &schema["type"] {
        Value::String(kind) => kind == "string",
        Value::Array(kinds) => kinds.iter().any(|kind| kind == "string"),
        _ => false,
    }
}

fn tag(tool: &str, parent: &str, schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    if let Some(Value::Object(properties)) = object.get_mut("properties") {
        for (key, property) in properties.iter_mut() {
            let clock = CLOCK_KEYS.contains(&key.as_str()) || span_bound(tool, parent, key);
            if clock && is_string(property) {
                property["format"] = Value::String("date-time".to_string());
            }
            tag(tool, key, property);
        }
    }
    for keyword in ["items", "additionalProperties", "if", "then", "else", "not"] {
        if let Some(inner) = object.get_mut(keyword).filter(|inner| inner.is_object()) {
            tag(tool, parent, inner);
        }
    }
    for keyword in ["oneOf", "anyOf", "allOf"] {
        for inner in object
            .get_mut(keyword)
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            tag(tool, parent, inner);
        }
    }
}
