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
                "items": string_schema("Exact scope id: local or about:<about>:dimension:<dimension_id>.")
            },
            "selectors": {
                "type": "array",
                "description": "All predicates must hold on the whole entry's key-to-values labels. mode/include/exclude/scope_ids instead keep an entry when any coordinate passes: exclude task is not task notexists. Hard filters hide evidence; read kmp_wake.labels first.",
                "items": label_selector_schema()
            },
            "scope": {
                "type": "string",
                "description": "Default current_about stays inside about. abouts reads the named list with separate ownership; selection neither merges abouts nor authorizes cross-about links. all_abouts scans every anchor, at a real cost.",
                "enum": ["current_about", "abouts", "all_abouts"]
            },
            "abouts": {
                "type": "array",
                "description": "Non-empty selection for scope=abouts. Include the current about if wanted.",
                "items": string_schema("Memory about id.")
            }
        }
    })
}
/// One label predicate, the shape every read's `dimensions.selectors` and the
/// view's `projection.labels` share, so an agent that filters a read filters
/// the loom with the same words.
pub(crate) fn label_selector_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["key", "op"],
        "properties": {
            "key": string_schema("Label key: the dimension kind (`task`, `agentic_process`, `incident`)."),
            "op": {
                "type": "string",
                "enum": ["in", "notin", "exists", "notexists"],
                "description": "in: any value under key is listed. notin: none is; an absent key passes. exists/notexists: key present/absent, with values empty."
            },
            "values": {
                "type": "array",
                "items": string_schema("Bare value from kmp_wake.labels; namespaced scope ids normalize to bare values.")
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
                "description": "Advisory cl100k planning hint, reported for compatibility; never filters structuredContent. max_bytes is the enforced ceiling."
            },
            "max_bytes": {
                "type": "integer",
                "minimum": 512,
                "default": 10_000,
                "description": "Enforced compact-JSON structuredContent ceiling. Below the stable response floor, return that floor with a warning naming it, not an error."
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
                "description": "Graph traversal depth; the default applies in embedded and live gRPC modes."
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
        "description": "Exactly one of time/ref; exclusive with interval. Only entries effective then on axis compete. Supersession and expiry are evaluated then: later changes do not invalidate the historical state.",
        "properties": {
            "time": string_schema("ISO-8601 instant to stand at."),
            "ref": string_schema("Memory ref whose instant on axis selects the historical state.")
        }
    })
}

pub(crate) fn interval_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Half-open [start,end) on axis, with at least one bound; exclusive with as_of. Only entries in the span compete, scored against its own collection. UNKNOWN reports the nearest outside match in proof.nearest_outside.",
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
        "description": "Clock for as_of/interval: occurred=event, observed=seen, ingested=written; validity=[valid_from,valid_until) overlaps the interval or contains the instant. Omitted: occurred, validity start, observed, ingested precedence, reported per entry. Explicit axis never substitutes another clock; applies only with as_of or interval."
    })
}
