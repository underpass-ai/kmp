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
        "Move the view by declaring what it should show — focus, clock axis, semantic zoom, dimensions, relation classes, selection, trace. Never pixels, coordinates or code. Atomic, idempotent, and under optimistic concurrency: if the person at the loom moved first, this conflicts and you rebase. Absence degrades rather than failing: a ref this store does not hold is dropped from the part that named it and listed in `unhonored`, and if none of the refs an intent names exist the view does not move at all and `applied` is false. Read `unhonored` — what is missing is always named there, never silently drawn.",
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
                    "properties": {"about": string_schema("Weave a different about. One this store does not hold is not honored: the loom stays on the about it was weaving and the ref is listed in `unhonored`.")}
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
                            "description": "Refs the view should frame. A ref this store does not hold is dropped and listed in `unhonored` — the loom never draws a placeholder that looks like data. If none of them exist the focus keeps the refs it had."
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
                        "abouts": {"type": "array", "maxItems": 5, "items": string_schema("Additional about to compare as a plane beside target.about; one this store does not hold is dropped and listed in `unhonored`."), "description": "Explicit additional contexts, at most five. Omit for the primary about alone."},
                        "dimensions": {"type": "array", "items": string_schema("Memory dimension to keep as a lane.")},
                        "labels": {
                            "type": "array",
                            "description": "Label predicates the loom filters its projection by, all of which must hold — the same `{ key, op, values }` every read takes under `dimensions.selectors`. Labels are ordinary attributes; a predicate keeps or drops whole entries without duplicating them across planes. A key or value absent from the primary and additional abouts' catalogues is reported as unhonored, never drawn as if it were data; read `kmp_wake`'s `labels` first.",
                            "items": label_selector_schema()
                        },
                        "relation_classes": {"type": "array", "items": semantic_class_schema()},
                        "overlays": {
                            "type": "array",
                            "items": string_schema("Observability series to align over the loom."),
                            "description": "Exact observability series to align above the loom on its current time axis. Missing backend series are reported by the viewer without inventing replacements."
                        }
                    }
                },
                "selection": {"type": ["string", "null"], "description": "Ref to select, or null to clear. A ref this store does not hold leaves the selection as it was and is listed in `unhonored`."},
                "trace": {
                    "type": ["object", "null"],
                    "additionalProperties": false,
                    "description": "The two ends of one claim, or null to clear. A trace needs both: if either end is not in this store the trace is left as it was and the absent end is listed in `unhonored`.",
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
