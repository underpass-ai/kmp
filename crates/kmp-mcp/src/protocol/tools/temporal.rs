//! `kmp_goto`, `kmp_near`, `kmp_rewind` and `kmp_forward` — one family.
//!
//! One concept: the four temporal moves differ only in their cursor argument
//! and the prose that says which way they walk. Writing them four times would
//! make a divergence between them look like a decision, so the family is built
//! once and parameterised by the cursor key.

use serde_json::{Value, json};

use crate::protocol::request_shape::{budget_schema, dimensions_schema};
use crate::protocol::response_shape::{
    page_output_schema, proof_output_schema, quality_output_schema, warnings_output_schema,
};
use crate::protocol::schema::{
    described, integer_schema, nullable_described, nullable_output_schema, output_object,
    string_array, string_schema,
};

/// The four moves, in the order the surface advertises them.
pub(in crate::protocol) fn definitions() -> Vec<Value> {
    vec![
        definition(
            "kmp_goto",
            "Jump to memory state at a timestamp, sequence, or ref. Cursor parameter: `at`. When the result is partial, response.next_action continues earlier history with kmp_rewind; feeding page.next_cursor back to kmp_goto does not paginate.",
            "at",
        ),
        definition(
            "kmp_near",
            "Return the temporal neighborhood around a timestamp, sequence, or ref. Cursor parameter: `around`. A partial neighborhood continues through response.next_action with kmp_rewind and kmp_forward; feeding page.next_cursor back to kmp_near does not paginate.",
            "around",
        ),
        definition(
            "kmp_rewind",
            "Move backward through memory from a timestamp, sequence, or ref. Entries within each page are newest-to-oldest, so concatenating continuation pages stays globally descending. Cursor parameter: `from`.",
            "from",
        ),
        definition(
            "kmp_forward",
            "Move forward through memory from a timestamp, sequence, or ref. Entries and continuation pages are oldest-to-newest. Cursor parameter: `from`.",
            "from",
        ),
    ]
}

pub(in crate::protocol) fn definition(name: &str, description: &str, cursor_key: &str) -> Value {
    let cursor_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "time": string_schema("ISO-8601 temporal cursor."),
            "sequence": {
                "type": "integer",
                "minimum": 1,
                "description": "Sequence within a temporal coordinate and dimension scope; it is not a store-global event number."
            },
            "ref": string_schema("Memory ref cursor.")
        }
    });
    let mut input_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["about", cursor_key],
        "properties": {
            "about": string_schema("Memory anchor or root ref to traverse from."),
            "axis": {
                "type": "string",
                "enum": ["occurred", "observed", "ingested", "validity"],
                "description": "Optional clock for this read. Omit it to preserve the compatible precedence (occurred, validity start, observed, ingested). An explicit axis never substitutes another clock."
            },
            "window": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "before_entries": {
                        "type": "integer",
                        "minimum": 0
                    },
                    "after_entries": {
                        "type": "integer",
                        "minimum": 0
                    }
                }
            },
            "limit": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "entries": {
                        "type": "integer",
                        "minimum": 1
                    },
                    "tokens": {
                        "type": "integer",
                        "minimum": 1
                    }
                }
            },
            "dimensions": dimensions_schema(),
            "include": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "evidence": {"type": "boolean"},
                    "relations": {"type": "boolean"},
                    "raw_refs": {
                        "type": "boolean",
                        "description": "Return typed raw audit refs for selected temporal entries."
                    }
                }
            },
            "depth": integer_schema("Optional graph traversal depth. Applies in embedded and live gRPC modes; it overrides budget.depth."),
            "budget": budget_schema(2_400, 3)
        }
    });
    input_schema["properties"][cursor_key] = cursor_schema;
    super::definition_with_output(
        name,
        description,
        input_schema,
        output_schema(name, cursor_key),
    )
}

pub(in crate::protocol) fn output_schema(tool_name: &str, cursor_key: &str) -> Value {
    let cursor_description = match tool_name {
        "kmp_goto" => "Boundary ref for earlier history. Do not pass it back to `at.ref`; follow top-level `next_action`, which continues with `kmp_rewind`.".to_string(),
        "kmp_near" => "Boundary ref for a partial neighborhood. Do not pass it back to `around.ref`; follow top-level `next_action`, which names the earlier and later moves.".to_string(),
        _ => format!("Memory-ref cursor for the next temporal slice; place it in `{cursor_key}.ref` while keeping the other arguments unchanged."),
    };
    output_object(json!({
        "summary": described("string", "Concise description of the temporal selection."),
        "next_action": nullable_described("string", "The exact temporal move that can consume a partial result's boundary cursor, or null when the selection is complete."),
        "temporal": nullable_described("object", "Resolved direction, selected clock axis, requested cursor, and resolved coordinate."),
        "coverage": output_object(json!({
            "requested": nullable_described("object", "Dimension selection requested by the caller."),
            "included": string_array("Dimension scope ids included in the result."),
            "missing": string_array("Requested dimension scope ids not present in the result."),
            "dimensions": described("array", "Per-dimension returned counts and presence flags.")
        })),
        "entries": described("array", "Temporal entries in traversal order, each with ref, kind, text, coordinates, and metadata."),
        "page": page_output_schema("temporal entries", &cursor_description),
        "raw_refs": described("array", "Typed raw audit refs for selected entries when include.raw_refs=true."),
        "proof": proof_output_schema("Temporal reads use medium when entries were returned and unknown when none were returned; this is not relation-writer certainty."),
        "quality": nullable_output_schema(quality_output_schema(), "Response-shape metrics; null when the backend supplied none."),
        "warnings": warnings_output_schema()
    }))
}
