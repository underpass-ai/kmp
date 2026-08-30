//! Request shapes more than one tool accepts.
//!
//! One concept: the argument blocks that recur across verbs — dimension
//! selection, a temporal cursor, a budget, a recall page. A shape only one
//! verb accepts belongs with that verb.
//!
//! Depends only on `schema`, never on a tool.

use serde_json::{Value, json};

use super::schema::{string_map_schema, string_schema};

pub(super) fn dimensions_schema() -> Value {
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
pub(super) fn temporal_coordinate_schema() -> Value {
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
pub(super) fn budget_schema(default_tokens: u32, default_depth: u32) -> Value {
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
                "description": "Normative maximum bytes for compact serialized structuredContent. Defaults to the host-safe 10,000-byte profile."
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
pub(super) fn recall_page_schema() -> Value {
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
