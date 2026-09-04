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
        "kmp_write_memory",
        "Write to memory. This is the writer to use: it validates intent and relation quality, then commits through canonical kmp_ingest. Normal writes are one call: omit `options.dry_run` or set it to false; validation failures write nothing. Set it to true only for an explicitly requested preview or payload debugging. Reach for kmp_ingest only when producing the exact graph yourself.",
        write_memory_schema(),
        write_memory_output_schema(),
    )
}

pub(crate) fn write_memory_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["about", "intent", "actor", "observed_at", "scope", "current"],
        "properties": {
            "about": string_schema("Memory anchor or root ref this semantic memory event should attach to."),
            "intent": {
                "type": "string",
                "description": "What this write records and therefore which planner rules apply. This is a write-operation intent, not the stored entry kind: record_delta requires semantic_delta, while current.kind describes the durable fact. record_summary attaches an English search summary to a memory that already exists: give current.ref and current.summary_en only — the text, kind and coordinates are read from the store and cannot be supplied, no connect_to is written, and a summary the lint refuses is rejected with every fault named. `kmp-mcp summaries pending` lists the memories that owe one.",
                "enum": [
                    "record_turn",
                    "record_observation",
                    "record_decision",
                    "record_feedback",
                    "record_delta",
                    "record_summary"
                ]
            },
            "actor": string_schema("Human, agent, or component producing the write."),
            "observed_at": string_schema("RFC3339 timestamp in UTC for provenance and default coordinates. UTC is required, not implied: RFC3339 permits an offset, and writers sending local wall-clock time with a `Z` put the memory's frontier hours into the future. A stamp more than five minutes ahead of the kernel's clock is refused \u{2014} read the real clock rather than composing one. Earlier times are fine: recording something that happened yesterday is a backfill, not an error."),
            "occurred_at": string_schema("Optional RFC3339 timestamp for when the recorded fact or event happened. Omit it when the writer does not know; observed_at is not a substitute."),
            "valid_from": string_schema("Optional RFC3339 start of the interval in which the recorded state is valid."),
            "valid_until": string_schema("Optional RFC3339 exclusive end of the interval in which the recorded state is valid."),
            "rank": {
                "type": "integer",
                "minimum": 1,
                "description": "Optional positive rank within each emitted coordinate."
            },
            "source_kind": {
                "type": "string",
                "enum": ["human", "agent", "projection", "derived"]
            },
            "scope": {
                "type": "object",
                "additionalProperties": false,
                "required": ["process"],
                "properties": {
                    "task": string_schema("Optional task dimension scope id."),
                    "process": string_schema("Caller-defined stable id for the agentic process this memory belongs to. Unknown values intentionally create or attach that process dimension; this is an identifier, not an enum."),
                    "episode": string_schema("Optional agentic episode dimension scope id.")
                }
            },
            "current": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "ref": string_schema("Optional stable memory entry ref. Omit it for a new memory so the writer planner generates a readable ref with a deterministic logical-write identity suffix. A supplied ref is an update address: it must be a safe descendant of this exact about (`{about}:...`) and can never target the about anchor, another about, an internal evidence/dimension id, or a path-shaped key. Exact retries keep the same generated ref; distinct writes cannot collapse merely because their summaries match or share a long prefix."),
                    "kind": {
                        "type": "string",
                        "description": "Semantic kind stored on the current entry. It is deliberately broader than intent: constraint, preference, derived_value, error_path, and success_path describe durable facts while intent describes the writer operation.",
                        "enum": [
                            "turn",
                            "observation",
                            "decision",
                            "feedback",
                            "semantic_delta",
                            "constraint",
                            "preference",
                            "derived_value",
                            "error_path",
                            "success_path"
                        ]
                    },
                    "summary": string_schema("Concise semantic memory text to store, in the language of the work. This is what kmp_ask cites, byte for byte."),
                    "summary_en": string_schema("English rendering of `summary` for search, written by you as you write the memory. kmp_ask searches it and never cites it: an English question reaches this memory through it, and the citation is `summary` byte for byte. Write plain English a reader would ask with, keep every number, identifier and acronym exactly as written (`v0.7.0`, `#469`, `kmp-mcp`, `ADR`), and never alter `summary` to fit it. Strict mode requires it when `summary` is not written in English, and refuses one that leans to another language, carries fewer than two informative words, repeats `summary` word for word, or drops an identifier `summary` carries; outside strict mode such a summary is stored and carries nothing. Worth writing for English text too when its wording is jargon (`rollout slipped` → `launch postponed`)."),
                    "evidence": string_schema("Direct evidence for the new memory entry. Required when options.strict is omitted or true; optional only when the caller explicitly sets options.strict=false.")
                }
            },
            "semantic_delta": {
                "type": "object",
                "additionalProperties": false,
                "required": ["from", "to", "why", "evidence"],
                "properties": {
                    "ref": string_schema("Optional stable semantic delta entry ref. Omit it for a new delta. Like current.ref, a supplied value is an update address and must be a safe descendant of this exact about (`{about}:...`); it cannot target the about anchor, another about, an internal evidence/dimension id, or a path-shaped key."),
                    "from": string_schema("Previous known state."),
                    "to": string_schema("New state."),
                    "why": string_schema("Why this state change is valid."),
                    "evidence": string_schema("Evidence proving the state change.")
                }
            },
            "connect_to": {
                "type": "array",
                "description": "Existing memory refs the new entry connects to. Omit this field or send an empty array only for the first strict write that creates a new about; once the about exists, strict runtime validation requires at least one relation.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["ref", "rel", "class"],
                    "properties": {
                        "ref": string_schema("Existing memory ref this new memory connects to. Inside this about for every relation; a ref of another about is accepted only with `same_event_as` or `same_entity_as`, class `evidential`, `why`, `evidence`, and the kmp_relate proposal in `read_context.relate_proposals`. That is the one relation that crosses an about; the edge lives in this about and the other does not change."),
                        "rel": {
                            "type": "string",
                            "enum": writer_relation_names(),
                            "description": relation_vocabulary_description(
                                "Relation type for this link."
                            )
                        },
                        "class": semantic_class_schema(),
                        "why": string_schema("Why this specific semantic connection holds and what a later reader should understand when traversing it. Required for non-structural relations."),
                        "evidence": string_schema("The concrete observation or source that supports the relation rationale. Required for non-structural relations."),
                        "confidence": {
                            "type": "string",
                            "enum": ["high", "medium", "low", "unknown"]
                        }
                    }
                }
            },
            "read_context": read_context_schema(),
            "idempotency_key": string_schema("Optional stable idempotency key. Omit to generate one from the write payload."),
            "options": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "dry_run": {
                        "type": "boolean",
                        "description": "When true, only return the compiled canonical kmp_ingest preview and write nothing. Defaults to false: the call commits."
                    },
                    "strict": {
                        "type": "boolean",
                        "description": "When true, fail fast on unsupported relations, missing proof, a memory not written in English that has no current.summary_en, and a current.summary_en that fails the lint. Defaults to true."
                    },
                    "sequence": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Explicit coordinate sequence. Omit it to let the kernel assign the next free sequence independently in every selected (dimension, scope) coordinate."
                    }
                }
            }
        },
        "allOf": [{
            "if": {
                "properties": {"intent": {"const": "record_summary"}},
                "required": ["intent"]
            },
            "then": {
                "properties": {
                    "current": {"required": ["ref", "summary_en"]}
                }
            },
            "else": {
                "properties": {
                    "current": {"required": ["kind", "summary"]}
                }
            }
        }, {
            "if": {
                "allOf": [
                    {
                        "not": {
                            "required": ["options"],
                            "properties": {
                                "options": {
                                    "required": ["strict"],
                                    "properties": {"strict": {"const": false}}
                                }
                            }
                        }
                    },
                    {"not": {"properties": {"intent": {"const": "record_summary"}}, "required": ["intent"]}}
                ]
            },
            "then": {
                "properties": {
                    "current": {"required": ["kind", "summary", "evidence"]}
                }
            }
        }]
    })
}

pub(crate) fn read_context_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Caller-supplied audit of which stored refs were read before choosing a relation. Strict validation checks that rich relation targets occur here; KMP cannot prove that a caller actually read them, so prior_context_observed is an asserted audit fact rather than a server observation.",
        "properties": {
            "inspected_refs": {
                "type": "array",
                "items": string_schema("Memory ref inspected with kmp_inspect before writing.")
            },
            "temporal_refs": {
                "type": "array",
                "items": string_schema("Memory ref observed through kmp_goto, kmp_near, kmp_rewind, or kmp_forward before writing.")
            },
            "wake_refs": {
                "type": "array",
                "items": string_schema("Memory ref observed in a kmp_wake packet before writing.")
            },
            "ask_refs": {
                "type": "array",
                "items": string_schema("Memory ref observed in deterministic kmp_ask proof/evidence before writing.")
            },
            "trace_paths": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["from", "to"],
                    "properties": {
                        "from": string_schema("Trace source ref observed before writing."),
                        "to": string_schema("Trace target ref observed before writing."),
                        "refs": {
                            "type": "array",
                            "items": string_schema("Optional intermediate ref observed in the trace path.")
                        }
                    }
                }
            },
            "relate_proposals": {
                "type": "array",
                "description": "Proposals kmp_relate returned, handed back as they came: the proof a writer carries when it declares an equivalence to a ref of another about.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["from", "to", "proposed_by"],
                    "properties": {
                        "from": string_schema("The proposal's `from`, as kmp_relate returned it."),
                        "to": string_schema("The proposal's `to`, as kmp_relate returned it."),
                        "proposed_by": {
                            "type": "array",
                            "minItems": 1,
                            "items": {
                                "type": "string",
                                "enum": ["identifier", "summary", "entity"]
                            }
                        }
                    }
                }
            }
        }
    })
}

fn write_memory_output_schema() -> Value {
    output_object(json!({
        "accepted": described("boolean", "True only when the canonical ingest was committed; false for a dry-run preview."),
        "dry_run": described("boolean", "Whether this response is a validated preview that wrote nothing."),
        "summary": described("string", "Counts and scope of the semantic write the planner prepared."),
        "generated_refs": string_array("Stable refs generated for entries whose ref the caller omitted. Their identity suffix is deterministic for an exact logical-write retry and distinct across different writes."),
        "relations": string_array("Typed relation names compiled into the canonical ingest."),
        "relation_quality": described("array", "Per-relation validation, including rich/anemic quality and prior-context evidence."),
        "relation_quality_metrics": described("object", "Aggregate counts and prior-context coverage for the compiled relations."),
        "ingest_preview": described("object", "Canonical kmp_ingest arguments. Present only on dry-run."),
        "ingest_result": described("object", "Canonical kmp_ingest result. Present only after a committed write."),
        "diagnostics": described("array", "Planner diagnostics that qualify the write."),
        "next_suggested_reads": string_array("Concrete refs worth reading next to verify or continue the write."),
        "viewer": output_object(json!({
            "url": described("string", "Loopback, read-only viewer URL carrying this session's capability."),
            "tell_the_user": described("string", "One-time handoff text for the human; it is not another kernel instruction.")
        }))
    }))
}
