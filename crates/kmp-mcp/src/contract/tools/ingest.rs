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
                                    "metadata": {
                                        "type": "object",
                                        "additionalProperties": {"type": "string"},
                                        "description": "Free string metadata stored beside the entry and searched by kmp_ask. One key is reserved: `summary_en`, an English rendering of `text` for search, searched and never cited. The kernel lints it and returns a warning for an entry whose summary leans to another language, carries fewer than two informative words, repeats `text` word for word, or drops an identifier `text` carries; such a summary is stored and carries nothing."
                                    }
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
                },
                "label_policy": {
                    "type": "string",
                    "enum": ["warn", "refuse"],
                    "description": "What to do with a dimension that resembles a label the about already holds — the same identifier up to case and separators, or the same value under another key. `warn` (the default) writes it and says so in `warnings` and `memory.resembling_labels`; `refuse` rejects the ingest naming both labels, unless the dimension carries the metadata `writer_intended_new: \"true\"`."
                }
            }
        }),
        ingest_output_schema(),
    )
}

fn ingest_output_schema() -> Value {
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
            "read_after_write_ready": described("boolean", "Whether a read issued now is guaranteed to observe this write."),
            "created_dimensions": string_array("Dimension nodes this ingest declared for the first time, namespaced `about:{about}:dimension:{scope}`: the labels the write created rather than reused."),
            "resembling_labels": described("array", "Labels this ingest declared that resemble one the about already holds, written under `label_policy: warn`: each with `key`, `value`, `existing_key`, `existing_value`, `kind` (`same_label_spelled_differently` or `value_under_another_key`) and `why`.")
        })),
        "warnings": warnings_output_schema()
    }))
}
