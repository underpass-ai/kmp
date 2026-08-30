//! `kmp_trace` — the proof path between two refs.
//!
//! One concept: what tracing accepts, including the page shape only it uses,
//! and the ordered path it answers with.

use serde_json::{Value, json};

use crate::protocol::request_shape::budget_schema;
use crate::protocol::response_shape::{
    page_output_schema, quality_output_schema, warnings_output_schema,
};
use crate::protocol::schema::{described, nullable_output_schema, output_object, string_schema};

pub(in crate::protocol) fn definition() -> Value {
    super::definition_with_output(
        "kmp_trace",
        "Trace the proof path between two memory refs owned by one explicit about. Both refs are rejected before traversal unless they belong to that about.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["about", "from", "to"],
            "properties": {
                "about": string_schema("Memory anchor that owns both refs."),
                "from": string_schema("Source memory ref. In live gRPC mode this must resolve to a kernel node id."),
                "to": string_schema("Target memory ref. In live gRPC mode this must resolve to a kernel node id."),
                "role": string_schema("Optional caller role."),
                "goal": string_schema("Optional trace goal."),
                "page": page_schema(),
                "budget": budget_schema(1_600, 1)
            }
        }),
        output_schema(),
    )
}

pub(in crate::protocol) fn page_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "entries": {
                "type": "integer",
                "minimum": 1,
                "description": "Maximum number of trace relations to return in this page."
            },
            "cursor": string_schema("Opaque cursor returned by page.next_cursor.")
        }
    })
}

pub(in crate::protocol) fn output_schema() -> Value {
    output_object(json!({
        "summary": described("string", "Concise statement of the path selection."),
        "trace": described("array", "Ordered typed relations connecting from to to; empty means no path in the same memory graph."),
        "page": page_output_schema("trace relations", "Opaque trace cursor; repeat it as page.cursor with every other argument unchanged."),
        "quality": nullable_output_schema(quality_output_schema(), "Response-shape metrics; null when the backend supplied none."),
        "warnings": warnings_output_schema()
    }))
}
