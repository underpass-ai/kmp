//! Request shapes more than one tool accepts.
//!
//! One concept: the argument blocks that recur across verbs — dimension
//! selection, a temporal cursor, a budget, a recall page. A shape only one
//! verb accepts belongs with that verb.
//!
//! Depends only on `schema`, never on a tool.

use serde_json::{Value, json};

use super::primitives::{string_map_schema, string_schema};

pub(crate) fn dimensions_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "mode": {
                "type": "string",
                "enum": ["all", "only", "except"]
            },
            "include": {
                "type": "array",
                "items": string_schema("Dimension kind to include.")
            },
            "exclude": {
                "type": "array",
                "items": string_schema("Dimension kind to exclude.")
            },
            "scope_ids": {
                "type": "array",
                "items": string_schema("Exact dimension scope id to include. Values may be local memory dimension ids or namespaced about:<about>:dimension:<dimension_id> ids.")
            },
            "selectors": {
                "type": "array",
                "description": "Predicates over the labels an entry stands in, all of which must hold. `mode`, `include`, `exclude` and `scope_ids` read one coordinate at a time and keep an entry when one of its coordinates passes; a selector reads the whole entry as key -> values, so `{key: task, op: notexists}` keeps only the entries with no task label, where `exclude: [task]` keeps every entry that also stands in a process. A hard filter, never a score: what it hides is invisible, so read `kmp_wake`'s `labels` first.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["key", "op"],
                    "properties": {
                        "key": string_schema("Label key: the dimension kind (`task`, `agentic_process`, `incident`)."),
                        "op": {
                            "type": "string",
                            "enum": ["in", "notin", "exists", "notexists"],
                            "description": "`in`: one of the entry's values under `key` is in `values`. `notin`: none is, and an entry without the key passes. `exists` / `notexists`: the key is present / absent; `values` must be empty."
                        },
                        "values": {
                            "type": "array",
                            "items": string_schema("Bare label value as `kmp_wake` lists it in `labels`; a namespaced scope id is read as its bare value.")
                        }
                    }
                }
            },
            "scope": {
                "type": "string",
                "description": "Which abouts this read may reach. `current_about` (the default) stays inside `about`. `abouts` reads the named list together — this is how one project's memory is read from another project's conversation, since abouts are never joined by relations. `all_abouts` sweeps every anchor, which is a real cost on a large store.",
                "enum": ["current_about", "abouts", "all_abouts"]
            },
            "abouts": {
                "type": "array",
                "description": "The abouts to read together when `scope` is `abouts`. Include the current one if you still want it.",
                "items": string_schema("Memory about id.")
            }
        }
    })
}
pub(crate) fn temporal_coordinate_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": true,
        "required": ["dimension", "scope_id"],
        "properties": {
            "dimension": string_schema("Dimension kind for this coordinate."),
            "scope_id": string_schema("Dimension scope id."),
            "occurred_at": string_schema("Optional RFC3339 occurrence timestamp."),
            "observed_at": string_schema("Optional RFC3339 observation timestamp, in UTC."),
            "ingested_at": string_schema("Optional RFC3339 ingest timestamp."),
            "valid_from": string_schema("Optional RFC3339 validity start."),
            "valid_until": string_schema("Optional RFC3339 validity end."),
            "sequence": {
                "type": "integer",
                "minimum": 1
            },
            "rank": {
                "type": "integer",
                "minimum": 1
            },
            "metadata": string_map_schema()
        }
    })
}
pub(crate) fn budget_schema(default_tokens: u32, default_depth: u32) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "tokens": {
                "type": "integer",
                "minimum": 1,
                "default": default_tokens,
                "description": format!("Advisory cl100k planning hint retained for compatibility and reported in the response; it does not filter structuredContent. Defaults to {default_tokens} for this verb. max_bytes is the normative host-safe ceiling.")
            },
            "max_bytes": {
                "type": "integer",
                "minimum": 512,
                "default": 10_000,
                "description": "Normative maximum bytes for compact serialized structuredContent. Defaults to the host-safe 10,000-byte profile. A ceiling below the response's stable floor returns the floor with a warning naming it, never an error."
            },
            "detail": {
                "type": "string",
                "enum": ["compact", "balanced", "full"],
                "default": "balanced",
                "description": "How much expansion detail is eligible before byte or entry caps are applied."
            },
            "depth": {
                "type": "integer",
                "minimum": 1,
                "default": default_depth,
                "description": format!("Graph traversal depth; defaults to {default_depth} for this verb in both embedded and live gRPC modes.")
            },
            "max_entries": {
                "type": "integer",
                "minimum": 1
            }
        }
    })
}
pub(crate) fn recall_page_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "entries": {
                "type": "integer",
                "minimum": 1,
                "description": "Optional maximum expansion items on this page; the normative byte ceiling still applies."
            },
            "cursor": {
                "type": "string",
                "minLength": 1,
                "description": "Opaque projection.page.next_cursor. Repeat all bound recall arguments unchanged; only page.entries, budget.tokens, and budget.max_bytes may vary."
            }
        }
    })
}

/// Where a recall stands in time, when the caller names it: the same three
/// arguments on `kmp_ask` and `kmp_wake`.
pub(crate) fn as_of_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Stand at one instant. Only what was in effect then on `axis` competes, and supersession and expiry are read as they stood then, so an entry replaced or expired later is current for this question. Exactly one of `time` or `ref`; exclusive with `interval`.",
        "properties": {
            "time": string_schema("ISO-8601 instant to stand at."),
            "ref": string_schema("Memory ref whose own instant on the axis the read stands at: what was in effect when that entry happened.")
        }
    })
}

pub(crate) fn interval_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Stand within a half-open span `[start, end)`. Only what falls inside on `axis` competes, the question's words are weighed against the span's own collection, and an UNKNOWN names the closest match outside the span in `proof.nearest_outside`. At least one bound; exclusive with `as_of`.",
        "properties": {
            "start": string_schema("ISO-8601 inclusive start. Omit it to leave the span open on this side."),
            "end": string_schema("ISO-8601 exclusive end. Omit it to leave the span open on this side.")
        }
    })
}

pub(crate) fn recall_axis_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["occurred", "observed", "ingested", "validity"],
        "description": "The clock `as_of` and `interval` read: when it happened, when it was seen, when it was written, or when it held (`validity`: the span `[valid_from, valid_until)` overlaps the interval, or contains the instant). Omit it to keep the compatible precedence (occurred, validity start, observed, ingested), which the proof names per entry. An explicit axis never substitutes another clock, and has nothing to select on without `as_of` or `interval`."
    })
}
