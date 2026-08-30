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
use crate::contract::schema::view_family::view_output_schema;
pub(crate) fn definition() -> Value {
    tool_definition_with_output(
        "kmp_view_apply_intent",
        "Move the view by declaring what it should show — focus, clock axis, semantic zoom, dimensions, relation classes, selection, trace. Never pixels, coordinates or code. Atomic, idempotent, and under optimistic concurrency: if the person at the loom moved first, this conflicts and you rebase.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["idempotency_key"],
            "properties": {
                "view_id": string_schema("Which view to move. Omit for the default one."),
                "expected_revision": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "The view_revision this intent was prepared against. Omit only when the move is unconditional; passing it is what stops an agent from yanking the loom out from under a person mid-gesture."
                },
                "idempotency_key": string_schema("A retried intent must be the same intent, not a second one. A replay answers with applied=false and the CURRENT state — success, not a conflict, because that intent already landed; read the state it returns to see whether the person has moved since."),
                "explanation": string_schema("Why, in the reader's terms. Shown to the human beside the change, because an agent may not rearrange what someone is looking at anonymously."),
                "actor": string_schema("Who is moving the view, for provenance. Defaults to `agent`."),
                "target": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {"about": string_schema("Weave a different about.")}
                },
                "focus": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "time_range": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "axis": {
                                    "type": "string",
                                    "enum": ["occurred", "observed", "ingested", "validity"],
                                    "description": "Which clock the loom's axis reads. KMP keeps several; say which one you mean."
                                },
                                "from": string_schema("ISO-8601 start of the window."),
                                "to": string_schema("ISO-8601 end of the window.")
                            }
                        },
                        "refs": {
                            "type": "array",
                            "items": string_schema("Memory ref to bring into focus."),
                            "description": "Refs the view should frame. Each must exist; the loom does not draw placeholders that look like data."
                        }
                    }
                },
                "projection": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "semantic_zoom": {
                            "type": "string",
                            "enum": ["atlas", "episode", "moment"],
                            "description": "Which rung of the ladder to show. The zoom changes representation, not just size."
                        },
                        "dimensions": {"type": "array", "items": string_schema("Memory dimension to keep as a lane.")},
                        "relation_classes": {"type": "array", "items": semantic_class_schema()},
                        "overlays": {
                            "type": "array",
                            "items": string_schema("Observability series to align over the loom."),
                            "description": "Exact observability series to align above the loom on its current time axis. Missing backend series are reported by the viewer without inventing replacements."
                        }
                    }
                },
                "selection": {"type": ["string", "null"], "description": "Ref to select, or null to clear."},
                "trace": {
                    "type": ["object", "null"],
                    "additionalProperties": false,
                    "properties": {
                        "from": string_schema("Where the claim starts."),
                        "to": string_schema("Where it should lead.")
                    }
                },
                "search": {"type": ["string", "null"], "description": "Query to highlight, or null to clear."}
            }
        }),
        view_output_schema(),
    )
}
