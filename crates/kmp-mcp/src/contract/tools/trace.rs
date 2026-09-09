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
                "about": string_schema("Memory anchor for this trace. Refs normally belong to it; a trace may cross a writer-declared same_event_as or same_entity_as link backed by a kmp_relate proposal. Naming another about's ref alone does not create a path."),
                "from": string_schema("Source memory ref. In live gRPC mode this must resolve to a kernel node id."),
                "to": string_schema("Target memory ref. In live gRPC mode this must resolve to a kernel node id."),
                "role": string_schema("Optional caller role."),
                "goal": string_schema("Optional trace goal."),
                "page": page_schema("Maximum number of trace relations to return in this page."),
                "budget": budget_schema(1_600, 1)
            }
        }),
        trace_output_schema(),
    )
}

fn trace_output_schema() -> Value {
    output_object(json!({
        "summary": described("string", "Concise statement of the path selection."),
        "trace": described("array", "Ordered typed relations connecting from to to; empty with page.has_more=false means no path in the same memory graph; otherwise proof remains pending."),
        "page": relation_page_output_schema("trace relations", "Opaque trace cursor; repeat it as page.cursor with selection arguments unchanged. budget.max_bytes and page.entries may vary; changed selected content or arguments return a conflict with a complete restart action."),
        "quality": nullable_output_schema(quality_output_schema(), "Response-shape metrics; null when the backend supplied none."),
        "next_actions": described("array", "Complete tool/arguments calls that continue this exact selection or raise an insufficient byte allowance; empty at the end. Execute without reconstructing filters or cursors."),
        "warnings": warnings_output_schema()
    }))
}
