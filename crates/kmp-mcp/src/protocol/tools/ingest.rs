//! `kmp_ingest` — the low-level batch writer.
//!
//! One concept: what this verb accepts and what it answers. The graph shape it
//! takes is described nowhere else, because no other verb takes it.

use serde_json::{Value, json};

use crate::protocol::relation_vocabulary::{
    relation_vocabulary_description, semantic_class_schema,
};
use crate::protocol::request_shape::temporal_coordinate_schema;
use crate::protocol::response_shape::warnings_output_schema;
use crate::protocol::schema::{described, output_object, string_map_schema, string_schema};

pub(in crate::protocol) fn definition() -> Value {
    super::definition_with_output(
        "kmp_ingest",
        "Low-level batch writer for callers producing the exact graph themselves. Prefer kmp_write_memory for ordinary agent writes because it validates intent and relation quality before compiling to this canonical ingest shape.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["about", "memory", "idempotency_key"],
            "properties": {
                "about": string_schema("Memory anchor or root ref this memory should attach to."),
                "memory": {
                    "type": "object",
                    "additionalProperties": true,
                    "required": ["dimensions", "entries"],
                    "properties": {
                        "dimensions": {
                            "type": "array",
                            "minItems": 1,
                            "items": {
                                "type": "object",
                                "additionalProperties": true,
                                "required": ["id", "kind"],
                                "properties": {
                                    "id": string_schema("Dimension scope id."),
                                    "kind": string_schema("Dimension kind."),
                                    "title": string_schema("Optional dimension title."),
                                    "metadata": string_map_schema()
                                }
                            }
                        },
                        "entries": {
                            "type": "array",
                            "minItems": 1,
                            "items": {
                                "type": "object",
                                "additionalProperties": true,
                                "required": ["id", "kind", "text", "coordinates"],
                                "properties": {
                                    "id": string_schema("Memory entry id. Must be a safe descendant of the exact about, beginning with `<about>:`; the about anchor and refs owned by another about are refused."),
                                    "kind": string_schema("Memory entry kind."),
                                    "text": string_schema("Memory entry text."),
                                    "coordinates": {
                                        "type": "array",
                                        "minItems": 1,
                                        "items": temporal_coordinate_schema()
                                    },
                                    "metadata": string_map_schema()
                                }
                            }
                        },
                        "relations": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": true,
                                "required": ["from", "to", "rel", "class"],
                                "properties": {
                                    "from": string_schema("Source ref owned by the exact about, its anchor, or a declared dimension id."),
                                    "to": string_schema("Target ref owned by the exact about, its anchor, or a declared dimension id."),
                                    "rel": {
                                        "type": "string",
                                        "minLength": 1,
                                        "description": relation_vocabulary_description(
                                            "Relationship type. Unknown extension types \
                                             are preserved but carry no writer spec; \
                                             prefer the cataloged vocabulary."
                                        )
                                    },
                                    "class": semantic_class_schema(),
                                    "why": string_schema("Optional rationale explaining why this specific semantic connection holds and what a later reader should understand when traversing it."),
                                    "evidence": string_schema("Optional concrete observation or source that supports the relation rationale."),
                                    "confidence": {
                                        "type": "string",
                                        "enum": ["high", "medium", "low", "unknown"]
                                    },
                                    "sequence": {
                                        "type": "integer",
                                        "minimum": 1
                                    },
                                    "motivation": string_schema("Optional motivation for the relation itself."),
                                    "method": string_schema("Optional method by which the relation holds."),
                                    "decision_id": string_schema("Optional about-owned decision ref associated with the relation."),
                                    "caused_by_node_id": string_schema("Optional about-owned causal predecessor ref."),
                                    "coordinate": temporal_coordinate_schema()
                                }
                            }
                        },
                        "evidence": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": true,
                                "required": ["id", "text"],
                                "properties": {
                                    "id": string_schema("Evidence id. Must begin with `evidence:<about>:` so it cannot overwrite evidence owned by another about."),
                                    "supports": {
                                        "type": "array",
                                        "items": string_schema("About-owned memory ref supported by this evidence, the about anchor, or a declared dimension id.")
                                    },
                                    "text": string_schema("Evidence text."),
                                    "source": string_schema("Evidence source."),
                                    "time": string_schema("Evidence timestamp."),
                                    "metadata": string_map_schema()
                                }
                            }
                        }
                    }
                },
                "provenance": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["source_kind", "source_agent", "observed_at"],
                    "properties": {
                        "source_kind": {
                            "type": "string",
                            "enum": ["human", "agent", "projection", "derived"]
                        },
                        "source_agent": string_schema("Agent or component that observed the memory."),
                        "observed_at": string_schema("RFC3339 observation timestamp, in UTC."),
                        "correlation_id": string_schema("Optional correlation id."),
                        "causation_id": string_schema("Optional causation id.")
                    }
                },
                "idempotency_key": string_schema("Required stable idempotency key for replay-safe ingest."),
                "dry_run": {
                    "type": "boolean"
                }
            }
        }),
        output_schema(),
    )
}

pub(in crate::protocol) fn output_schema() -> Value {
    output_object(json!({
        "summary": described("string", "Concise statement of what the kernel accepted."),
        "memory": output_object(json!({
            "about": described("string", "Memory anchor the write attached to."),
            "memory_id": described("string", "Stable id of the accepted memory event."),
            "accepted": output_object(json!({
                "entries": described("integer", "Number of entries accepted."),
                "relations": described("integer", "Number of relations accepted."),
                "evidence": described("integer", "Number of evidence items accepted.")
            })),
            "read_after_write_ready": described("boolean", "Whether a read issued now is guaranteed to observe this write.")
        })),
        "warnings": warnings_output_schema()
    }))
}
