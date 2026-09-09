use serde_json::{Value, json};

use crate::contract::schema::definition::tool_definition_with_output;
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
        "kmp_inspect",
        "Inspect one stored memory object or accepted write receipt inside an explicit about boundary. Receipt object.text is immutable command-detail JSON, not current memory. Evidence, links and raw records page under the byte ceiling; continuations can reuse the first page's object.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["about", "ref"],
            "properties": {
                "about": string_schema("Memory anchor that owns the inspected ref."),
                "ref": string_schema("Memory ref to inspect. In live gRPC mode this must resolve to a kernel node id."),
                "include": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "incoming": {
                            "type": "boolean",
                            "default": true,
                            "description": "Return direct typed relations whose target is this ref. Defaults to true; set false to narrow an oversized inspection."
                        },
                        "outgoing": {
                            "type": "boolean",
                            "default": true,
                            "description": "Return direct typed relations whose source is this ref. Defaults to true; set false to narrow an oversized inspection."
                        },
                        "details": {
                            "type": "boolean",
                            "default": true,
                            "description": "Return stored details, or receipt JSON with canonical_memory, writer diagnostics and command revision/hash. Defaults to true."
                        },
                        "raw": {
                            "type": "boolean",
                            "default": false,
                            "description": "Return typed raw audit refs for the inspected object."
                        }
                    }
                },
                "budget": inspect_budget_schema(),
                "page": inspect_page_schema()
            }
        }),
        inspect_output_schema(),
    )
}

fn inspect_output_schema() -> Value {
    output_object(json!({
        "summary": described("string", "Concise statement of which typed object was inspected."),
        "object": described("object", "Stored object with ref, kind, canonical text, metadata, and optional source. With page.repeat_object=false, contains only ref; reuse the first page's object."),
        "object_reused": described("boolean", "Present and true only when this continuation omits the unchanged object body. Retain the first page's full object; the cursor validates it against the current selection."),
        "links": output_object(json!({
            "incoming": described("array", "Direct typed relations whose target is the inspected ref."),
            "outgoing": described("array", "Direct typed relations whose source is the inspected ref.")
        })),
        "evidence": described("array", "Direct stored evidence for the object; text is canonical and supports names the refs it anchors."),
        "raw": described("array", "Typed raw audit records returned only when include.raw=true."),
        "page": output_object(json!({
            "offset": described("integer", "Number of expansion items reconstructed by earlier pages."),
            "returned": described("integer", "Number of expansion items returned on this page."),
            "total": described("integer", "Total evidence, outgoing-link, incoming-link and raw expansion items in the selection."),
            "has_more": described("boolean", "Whether expansion items remain after this page."),
            "next_cursor": nullable_described("string", "Opaque inspect cursor. Repeat the same bound arguments with this value in page.cursor."),
            "omitted": described("object", "Counts still remaining after this page, by details, evidence, outgoing, incoming and raw section."),
            "sections": described("object", "Per-section returned-on-page, remaining and total counts."),
            "required_bytes": described("integer", "Exact serialized bytes required by the complete inspection including its full object, even when this continuation reuses that object."),
            "minimum_progress_bytes": described("integer", "Present only when this page cannot return an expansion item. The next action offers at least this allowance, preferring the complete inspection up to the usual 10,000-byte budget. required_bytes still measures the complete inspection."),
            "guidance": nullable_described("string", "Continuation, narrowing and budget guidance when this response is partial; null for a complete first page.")
        })),
        "next_actions": {"type":"array","description":"Complete calls for the same inspection, including an increased byte allowance when no whole item fits. Execute in order and retain earlier pages; empty when complete.","items":output_object(json!({
            "tool": {"type":"string","const":"kmp_inspect"},
            "arguments": described("object", "The complete call, preserving about, ref, include and object reuse.")
        }))},
        "quality": nullable_output_schema(quality_output_schema(), "Response-shape metrics; null when the backend supplied none."),
        "warnings": warnings_output_schema()
    }))
}

fn inspect_budget_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "max_bytes": {
                "type": "integer",
                "minimum": 512,
                "default": 10_000,
                "description": "Compact-JSON structuredContent byte ceiling. Inspect pages expandable sections. If the stable object cannot fit, return its floor with a warning naming requested and required sizes; this is a qualified success, not a tool error."
            }
        }
    })
}

fn inspect_page_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "cursor": {
                "type": "string",
                "minLength": 1,
                "description": "Opaque cursor returned by inspect page.next_cursor. Repeat all bound arguments unchanged; budget.max_bytes may change."
            },
            "repeat_object": {
                "type": "boolean",
                "default": true,
                "description": "False requires page.cursor and retains only object.ref in this response. Reuse the full object from the first page; evidence and links still page verbatim. The cursor rejects a changed object or selection."
            }
        }
    })
}
