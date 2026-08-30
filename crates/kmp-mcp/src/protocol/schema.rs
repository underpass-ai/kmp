//! The JSON-Schema primitives every advertised schema is built from.
//!
//! One concept: the smallest shapes — a described scalar, a nullable one, an
//! array of strings, a free-form string map, an output object. Nothing here
//! knows which tool it is describing, which is why everything else can depend
//! on it and it depends on nothing.

use serde_json::{Value, json};

pub(super) fn string_map_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": {
            "type": "string"
        }
    })
}
/// An object schema whose public fields are complete. A response mapper that
/// grows without this schema growing with it is a protocol drift, not a
/// compatible extension: the whole point of `outputSchema` is that a caller
/// no longer has to guess what an unexplained field means.
pub(super) fn output_object(properties: Value) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": properties
    })
}
pub(super) fn described(kind: &str, description: &str) -> Value {
    json!({"type": kind, "description": description})
}
pub(super) fn nullable_described(kind: &str, description: &str) -> Value {
    json!({"type": [kind, "null"], "description": description})
}
pub(super) fn nullable_output_schema(mut schema: Value, description: &str) -> Value {
    schema["type"] = json!(["object", "null"]);
    schema["description"] = json!(description);
    schema
}
pub(super) fn string_array(description: &str) -> Value {
    json!({
        "type": "array",
        "description": description,
        "items": {"type": "string"}
    })
}
pub(super) fn string_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "description": description
    })
}
pub(super) fn integer_schema(description: &str) -> Value {
    json!({
        "type": "integer",
        "minimum": 1,
        "description": description
    })
}
