use serde_json::{Value, json};

use crate::contract::schema::definition::tool_definition_with_output;
use crate::contract::schema::paging::page_schema;
#[allow(unused_imports)]
use crate::contract::schema::primitives::*;
#[allow(unused_imports)]
use crate::contract::schema::relation_vocabulary::*;
#[allow(unused_imports)]
use crate::contract::schema::request_shape::*;
#[allow(unused_imports)]
use crate::contract::schema::response_shape::*;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    tool_definition_with_output(
        "kmp_trace",
        "Trace the proof path between two memory refs owned by one explicit about. Both refs are rejected before traversal unless they belong to that about, or are reachable from it through a declared equivalence (`same_event_as`, `same_entity_as`): the one edge that crosses an about, walked here.",
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
        trace_output_schema(),
    )
}

fn trace_output_schema() -> Value {
    output_object(json!({
        "summary": described("string", "Concise statement of the path selection."),
        "trace": described("array", "Ordered typed relations connecting from to to; empty means no path in the same memory graph."),
        "page": page_output_schema("trace relations", "Opaque trace cursor; repeat it as page.cursor with every other argument unchanged."),
        "quality": nullable_output_schema(quality_output_schema(), "Response-shape metrics; null when the backend supplied none."),
        "warnings": warnings_output_schema()
    }))
}
