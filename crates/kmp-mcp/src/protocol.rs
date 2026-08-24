use kmp_domain::KnownMemoryRelationType;
use serde_json::{Value, json};

use crate::tool_error::{ToolError, ToolErrorCode};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "underpass-kmp-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) fn initialize_result(backend: &str, grpc_tls: &str) -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION
        },
        "metadata": {
            "backend": backend,
            "grpc_tls": grpc_tls
        }
    })
}

pub(crate) fn tools_list_result() -> Value {
    json!({
        // The codes an agent may branch on, with what to do about each. They
        // were enumerated only in the source, while the skill told agents to
        // read the code — advice with nothing behind it in any host that does
        // not ship the skill.
        "_meta": {
            "kmp/errorCodes": ToolErrorCode::ALL
                .iter()
                .map(|code| json!({"code": code.as_str(), "means": code.guidance()}))
                .collect::<Vec<_>>()
        },
        "tools": [
            tool_definition(
                "kernel_ingest",
                "Submit memory with dimensions, entries, relations, evidence, and provenance for later traversal.",
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
                                            "id": string_schema("Memory entry id."),
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
                                            "from": string_schema("Source memory entry id."),
                                            "to": string_schema("Target memory entry id."),
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
                                            "decision_id": string_schema("Optional decision ref associated with the relation."),
                                            "caused_by_node_id": string_schema("Optional causal predecessor ref."),
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
                                            "id": string_schema("Evidence id."),
                                            "supports": {
                                                "type": "array",
                                                "items": string_schema("Memory ref supported by this evidence.")
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
                })
            ),
            tool_definition(
                "kernel_write_memory",
                "Write to memory. This is the writer to use: it validates intent and relation quality, then compiles to canonical kernel_ingest, and `options.dry_run` shows what a write would commit before committing it. Reach for kernel_ingest only when producing the exact graph yourself.",
                write_memory_schema()
            ),
            tool_definition(
                "kernel_wake",
                "Return a compact Kernel Memory Protocol wake packet for continuing work from memory.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["about"],
                    "properties": {
                        "about": string_schema("Memory anchor or root ref to wake from."),
                        "role": string_schema("Optional caller role."),
                        "intent": string_schema("Optional continuation intent."),
                        "dimensions": dimensions_schema(),
                        "depth": integer_schema("Optional graph traversal depth for live gRPC mode."),
                        "budget": budget_schema(),
                        "page": recall_page_schema()
                    }
                })
            ),
            tool_definition(
                "kernel_ask",
                "Retrieve stored evidence bearing on a question, or UNKNOWN. Nothing is generated: `answer` names what was retrieved and the text lives in `proof.evidence[].text` — read it, and judge whether it answers. `proof.confidence` is lexical term overlap between the question and the best-matching evidence item; it is not a judgement that the evidence answers, and it is not the `confidence` on a relation, which is writer certainty. UNKNOWN means memory did not answer; `summary` says whether nothing was retrieved or nothing retrieved bore on the question.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["about", "question"],
                    "properties": {
                        "about": string_schema("Memory anchor or root ref to ask from."),
                        "question": string_schema("Natural-language question."),
                        "answer_policy": {
                            "type": "string",
                            "description": "Deterministic evidence policy. show_conflicts surfaces explicit conflict relations in proof.conflicts; best_effort does not generate fallback text.",
                            "enum": ["evidence_or_unknown", "show_conflicts", "best_effort"]
                        },
                        "dimensions": dimensions_schema(),
                        "depth": integer_schema("Optional graph traversal depth for live gRPC mode."),
                        "budget": budget_schema(),
                        "page": recall_page_schema()
                    }
                })
            ),
            temporal_tool_definition(
                "kernel_goto",
                "Jump to memory state at a timestamp, sequence, or ref. Cursor parameter: `at`.",
                "at",
            ),
            temporal_tool_definition(
                "kernel_near",
                "Return the temporal neighborhood around a timestamp, sequence, or ref. Cursor parameter: `around`.",
                "around",
            ),
            temporal_tool_definition(
                "kernel_rewind",
                "Move backward through memory from a timestamp, sequence, or ref. Cursor parameter: `from`.",
                "from",
            ),
            temporal_tool_definition(
                "kernel_forward",
                "Move forward through memory from a timestamp, sequence, or ref. Cursor parameter: `from`.",
                "from",
            ),
            tool_definition(
                "kernel_trace",
                "Trace the proof path between two memory refs.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["from", "to"],
                    "properties": {
                        "from": string_schema("Source memory ref. In live gRPC mode this must resolve to a kernel node id."),
                        "to": string_schema("Target memory ref. In live gRPC mode this must resolve to a kernel node id."),
                        "role": string_schema("Optional caller role."),
                        "goal": string_schema("Optional trace goal."),
                        "page": page_schema(),
                        "budget": budget_schema()
                    }
                })
            ),
            tool_definition(
                "kernel_inspect",
                "Inspect the typed stored memory object, direct links, and evidence for one ref.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["ref"],
                    "properties": {
                        "ref": string_schema("Memory ref to inspect. In live gRPC mode this must resolve to a kernel node id."),
                        "include": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "incoming": {"type": "boolean"},
                                "outgoing": {"type": "boolean"},
                                "details": {"type": "boolean"},
                                "raw": {
                                    "type": "boolean",
                                    "description": "Return typed raw audit refs for the inspected object."
                                }
                            }
                        }
                    }
                })
            )
        ]
    })
}

fn write_memory_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["about", "intent", "actor", "observed_at", "scope", "current", "connect_to"],
        "properties": {
            "about": string_schema("Memory anchor or root ref this semantic memory event should attach to."),
            "intent": {
                "type": "string",
                "enum": [
                    "record_turn",
                    "record_observation",
                    "record_decision",
                    "record_feedback",
                    "record_delta"
                ]
            },
            "actor": string_schema("Human, agent, or component producing the write."),
            "observed_at": string_schema("RFC3339 timestamp in UTC for provenance and default coordinates. UTC is required, not implied: RFC3339 permits an offset, and writers sending local wall-clock time with a `Z` put the memory's frontier hours into the future. A stamp more than five minutes ahead of the kernel's clock is refused \u{2014} read the real clock rather than composing one. Earlier times are fine: recording something that happened yesterday is a backfill, not an error."),
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
                    "process": string_schema("Required agentic process dimension scope id."),
                    "episode": string_schema("Optional agentic episode dimension scope id.")
                }
            },
            "current": {
                "type": "object",
                "additionalProperties": false,
                "required": ["kind", "summary"],
                "properties": {
                    "ref": string_schema("Optional stable memory entry ref. Omit to let the writer planner generate one deterministically."),
                    "kind": {
                        "type": "string",
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
                    "summary": string_schema("Concise semantic memory text to store."),
                    "evidence": string_schema("Direct evidence for the new memory entry. Required in strict mode.")
                }
            },
            "semantic_delta": {
                "type": "object",
                "additionalProperties": false,
                "required": ["from", "to", "why", "evidence"],
                "properties": {
                    "ref": string_schema("Optional stable semantic delta entry ref."),
                    "from": string_schema("Previous known state."),
                    "to": string_schema("New state."),
                    "why": string_schema("Why this state change is valid."),
                    "evidence": string_schema("Evidence proving the state change.")
                }
            },
            "connect_to": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["ref", "rel", "class"],
                    "properties": {
                        "ref": string_schema("Existing memory ref this new memory connects to."),
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
                        "description": "When true, only return the compiled canonical kernel_ingest preview and write nothing. Defaults to false: the call commits."
                    },
                    "strict": {
                        "type": "boolean",
                        "description": "When true, fail fast on unsupported relations and missing proof. Defaults to true."
                    },
                    "sequence": {
                        "type": "integer",
                        "minimum": 1
                    }
                }
            }
        }
    })
}

fn read_context_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "inspected_refs": {
                "type": "array",
                "items": string_schema("Memory ref inspected with kernel_inspect before writing.")
            },
            "temporal_refs": {
                "type": "array",
                "items": string_schema("Memory ref observed through kernel_goto, kernel_near, kernel_rewind, or kernel_forward before writing.")
            },
            "wake_refs": {
                "type": "array",
                "items": string_schema("Memory ref observed in a kernel_wake packet before writing.")
            },
            "ask_refs": {
                "type": "array",
                "items": string_schema("Memory ref observed in deterministic kernel_ask proof/evidence before writing.")
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
            }
        }
    })
}

fn temporal_tool_definition(name: &str, description: &str, cursor_key: &str) -> Value {
    let cursor_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "time": string_schema("ISO-8601 temporal cursor."),
            "sequence": {
                "type": "integer",
                "minimum": 1
            },
            "ref": string_schema("Memory ref cursor.")
        }
    });
    let mut input_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["about", cursor_key],
        "properties": {
            "about": string_schema("Memory anchor or root ref to traverse from."),
            "window": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "before_entries": {
                        "type": "integer",
                        "minimum": 0
                    },
                    "after_entries": {
                        "type": "integer",
                        "minimum": 0
                    }
                }
            },
            "limit": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "entries": {
                        "type": "integer",
                        "minimum": 1
                    },
                    "tokens": {
                        "type": "integer",
                        "minimum": 1
                    }
                }
            },
            "dimensions": dimensions_schema(),
            "include": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "evidence": {"type": "boolean"},
                    "relations": {"type": "boolean"},
                    "raw_refs": {
                        "type": "boolean",
                        "description": "Return typed raw audit refs for selected temporal entries."
                    }
                }
            },
            "depth": integer_schema("Optional graph traversal depth for live gRPC mode."),
            "budget": budget_schema()
        }
    });
    input_schema["properties"][cursor_key] = cursor_schema;
    tool_definition(name, description, input_schema)
}

fn page_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "entries": {
                "type": "integer",
                "minimum": 1,
                "description": "Maximum number of trace relations to return in this page."
            },
            "cursor": string_schema("Opaque cursor returned by page.next_cursor.")
        }
    })
}

fn dimensions_schema() -> Value {
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

fn temporal_coordinate_schema() -> Value {
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

fn string_map_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": {
            "type": "string"
        }
    })
}

fn tool_definition(name: &str, description: &str, input_schema: Value) -> Value {
    tool_definition_with_output(name, description, input_schema, Value::Null)
}

/// A tool, with the shape of what it answers.
///
/// Inputs were described field by field and the response — the half the agent
/// actually reasons over — was described nowhere. `proof.confidence`,
/// `proof.superseded` against `proof.conflicts`, `page.total`,
/// `projection.next_action`, `resume_cursor`: every one of them arrived
/// unexplained, and what did explain them was `SKILL.md`, a Claude Code plugin
/// file that an agent in any other host never sees.
///
/// A memory kernel whose contract is only legible inside one vendor's plugin
/// is not a protocol.
fn tool_definition_with_output(
    name: &str,
    description: &str,
    input_schema: Value,
    output_schema: Value,
) -> Value {
    let mut definition = json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "_meta": {
            "anthropic/maxResultSizeChars": 10_000
        }
    });
    if !output_schema.is_null() {
        definition["outputSchema"] = output_schema;
    }
    definition
}

/// An object schema whose properties are described and whose extra fields are
/// tolerated: the response may grow, and a client that refuses a new field is
/// worse off than one that ignores it.
fn output_object(properties: Value) -> Value {
    json!({
        "type": "object",
        "additionalProperties": true,
        "properties": properties
    })
}

fn described(kind: &str, description: &str) -> Value {
    json!({"type": kind, "description": description})
}

/// `page`, with what `total` counts said out loud.
///
/// It counts different things in different verbs — expansion items in a
/// recall, temporal entries in a move — and nothing in the surface said the
/// unit changed. A number whose meaning the receiver has to guess is worse
/// than no number, because it will be acted on.
fn page_output_schema(unit: &str) -> Value {
    output_object(json!({
        "returned": described("integer", "How many items this response carries."),
        "total": described("integer", &format!("How many {unit} the selection holds in total.")),
        "has_more": described(
            "boolean",
            "Whether the slice was cut. A partial answer reported as a whole one is the failure \
             this field exists to prevent."
        ),
        "next_cursor": described(
            "string",
            "Opaque cursor for the next page, or null. Repeat every other argument unchanged."
        )
    }))
}

/// `proof`, which is where a caller decides whether to believe the answer.
fn proof_output_schema() -> Value {
    output_object(json!({
        "confidence": described(
            "string",
            "high | medium | low | unknown. Derived from lexical term overlap between the \
             question and the best-matching evidence item. It is not a judgement that the \
             evidence answers the question, and it is not the `confidence` on a relation, which \
             is writer certainty."
        ),
        "evidence": described(
            "array",
            "The stored evidence, verbatim. `text` is the canonical body — this is where the \
             answer actually is."
        ),
        "missing": described(
            "array",
            "What was looked for and not found. Non-empty alongside UNKNOWN, and it says which \
             kind: nothing retrieved at all, or nothing that bears on the question."
        ),
        "superseded": described(
            "array",
            "Entries a later one replaced, each with `superseded_by` and the `why`. A lifecycle, \
             not a disagreement: read the older entry as what was true then, not as advice."
        ),
        "conflicts": described(
            "array",
            "Entries that explicitly contradict each other and are both still live. The tension \
             is the information — this is deliberately not the same field as `superseded`."
        ),
        "matched_relations": described(
            "array",
            "Which typed relations contributed to the ordering. Relation prose can improve a \
             match and can never promote unrelated evidence into an answer."
        ),
        "matched_terms": described("array", "Question terms that matched retrieved evidence."),
        "path": described("array", "The traversal that connects the cited evidence."),
        "frontier_size": described(
            "integer",
            "How much was reachable and not returned, which is the signal to expand."
        )
    }))
}

/// `projection`, the budget envelope on a recall.
fn projection_output_schema() -> Value {
    output_object(json!({
        "contract": described("string", "The projection contract version, e.g. kmp.recall.projection.v1."),
        "budget": described("object", "The ceilings that applied, and the bytes actually used."),
        "detail": described("string", "compact | balanced | full — the detail tier that was served."),
        "excluded_by_detail": described(
            "integer",
            "Items a richer `budget.detail` would have included. Not a truncation: they were \
             never eligible at this tier."
        ),
        "next_action": described(
            "string",
            "The exact call that continues this page, or null when there is nothing after it."
        ),
        "page": page_output_schema("expansion items")
    }))
}

fn quality_output_schema() -> Value {
    output_object(json!({
        "causal_density": described(
            "number",
            "Share of returned relations that explain rather than merely connect. Low means the \
             memory is a list; it is a property of what was written, not of this call."
        ),
        "detail_coverage": described("number", "Share of returned nodes that carry stored detail."),
        "truncated": described("boolean", "Whether the rendering dropped anything.")
    }))
}

fn string_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "description": description
    })
}

fn integer_schema(description: &str) -> Value {
    json!({
        "type": "integer",
        "minimum": 1,
        "description": description
    })
}

fn writer_relation_names() -> Vec<&'static str> {
    KnownMemoryRelationType::writer_relation_types()
        .iter()
        .map(|relation_type| relation_type.as_str())
        .collect()
}

/// The relation vocabulary, projected from the kernel's own writer spec so
/// this documentation can never drift from what the kernel validates. The
/// relation is where KMP carries the why; a model that only sees a bare enum
/// writes connected-but-unexplained memory, which is the failure mode the
/// spec exists to prevent.
fn relation_vocabulary_description(header: &str) -> String {
    let mut description = format!(
        "{header} The relation carries the explanation: non-structural classes require why, \
         evidence and confidence. Prefer rich types — anemic types are an honest fallback for \
         when no richer semantic dependency can be proven, never a default. Vocabulary \
         (quality; allowed classes; when to use):"
    );
    for spec in KnownMemoryRelationType::writer_relation_types()
        .iter()
        .filter_map(|relation_type| relation_type.writer_spec())
    {
        let classes = spec
            .allowed_classes()
            .iter()
            .map(|class| class.as_str())
            .collect::<Vec<_>>()
            .join("|");
        description.push_str(&format!(
            " {} ({}; {}; {}).",
            spec.relation_type().as_str(),
            spec.quality().as_str(),
            classes,
            spec.reason()
        ));
    }
    description
}

fn semantic_class_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["structural", "causal", "motivational", "procedural", "evidential", "constraint"],
        "description": "What the link explains: structural = containment/membership, no proof \
                        required; causal = one memory triggered or produced another; \
                        motivational = one memory justifies or authorizes another; procedural = \
                        how something was executed, or plain succession; evidential = validates, \
                        proves, contradicts or verifies; constraint = limits or shapes another \
                        memory."
    })
}

fn budget_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "tokens": {
                "type": "integer",
                "minimum": 1,
                "description": "Advisory cl100k planning ceiling retained for compatibility; max_bytes is the normative host-safe ceiling."
            },
            "max_bytes": {
                "type": "integer",
                "minimum": 512,
                "description": "Normative maximum bytes for compact serialized structuredContent. Defaults to the host-safe 10,000-byte profile."
            },
            "detail": {
                "type": "string",
                "enum": ["compact", "balanced", "full"]
            },
            "depth": {
                "type": "integer",
                "minimum": 1
            },
            "max_entries": {
                "type": "integer",
                "minimum": 1
            }
        }
    })
}

fn recall_page_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "entries": {
                "type": "integer",
                "minimum": 1,
                "description": "Optional maximum expansion items on this page; byte and advisory-token ceilings still apply."
            },
            "cursor": {
                "type": "string",
                "minLength": 1,
                "description": "Opaque projection.page.next_cursor. Repeat all bound recall arguments unchanged; only page.entries, budget.tokens, and budget.max_bytes may vary."
            }
        }
    })
}

pub(crate) fn tool_success_result(structured_content: Value) -> Value {
    // `structuredContent` is the canonical response. Repeating the entire
    // pretty-printed JSON in the text block doubled every tool result and was
    // enough to overflow hosts even after the structured packet was budgeted.
    let text = structured_content
        .get("summary")
        .and_then(Value::as_str)
        .or_else(|| structured_content.get("answer").and_then(Value::as_str))
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            serde_json::to_string(&structured_content)
                .expect("fixture JSON should serialize as compact text")
        });
    json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ],
        "structuredContent": structured_content,
        "isError": false
    })
}

/// Applies the strictness the schemas already declare.
///
/// Every one of the ten tools says `"additionalProperties": false` and nothing
/// enforced it, which made the surface a silent-failure generator: a
/// misspelled `dimensions`, a `budget` nested one level too deep, a `from`
/// sent to `kernel_goto` where the cursor is `at` — each accepted, dropped,
/// and answered with a well-formed success built from defaults. The agent has
/// no way to tell a request that was honoured from one that was discarded, so
/// it reads the result as proof its arguments were understood and makes the
/// same call again.
///
/// The check reads the published schema rather than a second list, so it
/// cannot drift from what `tools/list` promises.
pub(crate) fn reject_unknown_arguments(tool: &str, arguments: &Value) -> Result<(), ToolError> {
    let Some(schema) = tool_input_schema(tool) else {
        return Ok(());
    };
    check_against_schema(schema, arguments, tool)
}

/// The schemas, built once.
///
/// This runs on every tool call, and `tools_list_result()` builds the whole
/// ten-tool document — relation vocabulary included — from scratch each time.
/// Rebuilding a document that cannot change, per call, to read one field of
/// it, is a cost with nothing on the other side of it.
fn tool_input_schema(tool: &str) -> Option<&'static Value> {
    static SCHEMAS: std::sync::OnceLock<std::collections::BTreeMap<String, Value>> =
        std::sync::OnceLock::new();
    SCHEMAS
        .get_or_init(|| {
            tools_list_result()["tools"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|definition| {
                    Some((
                        definition["name"].as_str()?.to_string(),
                        definition["inputSchema"].clone(),
                    ))
                })
                .collect()
        })
        .get(tool)
}

fn check_against_schema(schema: &Value, value: &Value, path: &str) -> Result<(), ToolError> {
    let (Some(properties), Some(object)) = (schema["properties"].as_object(), value.as_object())
    else {
        return Ok(());
    };

    if schema["additionalProperties"] == Value::Bool(false) {
        for key in object.keys() {
            if properties.contains_key(key) {
                continue;
            }
            let known = properties.keys().cloned().collect::<Vec<_>>().join(", ");
            return Err(ToolError::invalid_argument(format!(
                "`{path}` has no argument `{key}`. This call would otherwise have been answered \
                 with that argument silently dropped. Accepted here: {known}."
            )));
        }
    }

    for (key, nested) in object {
        let Some(nested_schema) = properties.get(key) else {
            continue;
        };
        let nested_path = format!("{path}.{key}");
        check_against_schema(nested_schema, nested, &nested_path)?;
        if let (Some(items), Some(array)) = (nested_schema.get("items"), nested.as_array()) {
            for entry in array {
                check_against_schema(items, entry, &nested_path)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn tool_error_result(error: &ToolError) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": error.message
            }
        ],
        "structuredContent": {
            "error": {
                "code": error.code.as_str(),
                "message": error.message
            }
        },
        "isError": true
    })
}

pub(crate) fn jsonrpc_result(id: Value, result: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
    .to_string()
}

pub(crate) fn jsonrpc_error(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_result_reports_backend_metadata() {
        let result = initialize_result("stub", "mutual");

        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], SERVER_NAME);
        assert_eq!(result["metadata"]["backend"], "stub");
        assert_eq!(result["metadata"]["grpc_tls"], "mutual");
    }

    #[test]
    fn tools_list_result_exposes_expected_tool_shapes() {
        let result = tools_list_result();
        let tools = result["tools"]
            .as_array()
            .expect("tools should be an array");

        assert_eq!(tools.len(), 10);
        assert_eq!(tools[0]["name"], "kernel_ingest");
        assert_eq!(tools[0]["inputSchema"]["required"][1], "memory");
        assert_eq!(tools[1]["name"], "kernel_write_memory");
        assert_eq!(tools[1]["inputSchema"]["required"][1], "intent");
        assert_eq!(
            tools[1]["inputSchema"]["properties"]["connect_to"]["items"]["properties"]["rel"]["enum"]
                [0],
            "follows"
        );
        assert!(
            tools[1]["inputSchema"]["properties"]
                .get("read_context")
                .is_some()
        );
        let why_description = tools[1]["inputSchema"]["properties"]["connect_to"]["items"]
            ["properties"]["why"]["description"]
            .as_str()
            .expect("writer why carries operational guidance");
        let evidence_description =
            tools[1]["inputSchema"]["properties"]["connect_to"]["items"]["properties"]["evidence"]
                ["description"]
                .as_str()
                .expect("writer evidence carries operational guidance");
        assert!(why_description.contains("specific semantic connection"));
        assert!(why_description.contains("later reader"));
        assert!(evidence_description.contains("concrete observation or source"));
        assert!(evidence_description.contains("relation rationale"));
        assert_eq!(tools[2]["name"], "kernel_wake");
        assert_eq!(tools[2]["inputSchema"]["required"][0], "about");
        assert_eq!(tools[2]["_meta"]["anthropic/maxResultSizeChars"], 10_000);
        assert_eq!(
            tools[2]["inputSchema"]["properties"]["budget"]["properties"]["max_bytes"]["minimum"],
            512
        );
        assert!(tools[2]["inputSchema"]["properties"].get("page").is_some());
        assert_eq!(tools[3]["name"], "kernel_ask");
        assert_eq!(tools[3]["inputSchema"]["required"][1], "question");
        assert_eq!(tools[3]["_meta"]["anthropic/maxResultSizeChars"], 10_000);
        assert!(tools[3]["inputSchema"]["properties"].get("page").is_some());
        assert!(
            tools[3]["inputSchema"]["properties"]
                .get("prefer")
                .is_none()
        );
        assert_eq!(tools[4]["name"], "kernel_goto");
        assert_eq!(tools[4]["inputSchema"]["required"][1], "at");
    }

    /// The tool documentation is generated from the writer spec; this pins
    /// that every cataloged type appears with its quality tier, in both the
    /// writer's and the batch surface, so a model reading `tools/list` learns
    /// the vocabulary the kernel will actually validate.
    #[test]
    fn relation_vocabulary_documentation_matches_the_writer_spec() {
        let tools = tools_list_result();
        let writer_doc = tools["tools"][1]["inputSchema"]["properties"]["connect_to"]["items"]
            ["properties"]["rel"]["description"]
            .as_str()
            .expect("writer rel carries generated documentation")
            .to_string();
        let ingest_doc = tools["tools"][0]["inputSchema"]["properties"]["memory"]["properties"]
            ["relations"]["items"]["properties"]["rel"]["description"]
            .as_str()
            .expect("ingest rel carries generated documentation")
            .to_string();

        for relation_type in KnownMemoryRelationType::writer_relation_types() {
            let spec = relation_type
                .writer_spec()
                .expect("writer relation types carry a spec");
            for doc in [&writer_doc, &ingest_doc] {
                assert!(
                    doc.contains(&format!(
                        "{} ({};",
                        spec.relation_type().as_str(),
                        spec.quality().as_str()
                    )),
                    "documentation names `{}` with its quality tier",
                    spec.relation_type().as_str()
                );
            }
        }
        assert!(
            writer_doc.contains("anemic types are an honest fallback"),
            "documentation states the anemic-fallback doctrine"
        );
    }

    #[test]
    fn tool_results_are_mcp_content_blocks() {
        let success = tool_success_result(json!({"answer": "Austin"}));
        assert_eq!(success["isError"], false);
        assert_eq!(success["structuredContent"]["answer"], "Austin");
        assert!(
            success["content"][0]["text"]
                .as_str()
                .expect("content text should be present")
                .contains("Austin")
        );

        let error = tool_error_result(&ToolError::backend("no evidence"));
        assert_eq!(error["isError"], true);
        assert_eq!(error["content"][0]["text"], "no evidence");
        assert_eq!(error["structuredContent"]["error"]["code"], "backend_error");

        let missing = tool_error_result(&ToolError::not_found("node `question:missing` not found"));
        assert_eq!(missing["structuredContent"]["error"]["code"], "not_found");
    }

    /// The property the substring matcher could not have. Same words, two
    /// codes, because the producer chose and the words were never consulted.
    #[test]
    fn the_code_comes_from_the_producer_and_not_from_the_message() {
        let phrased_like_a_bad_argument =
            "the store must be migrated before it can be opened; this is invalid";
        assert_eq!(
            tool_error_result(&ToolError::backend(phrased_like_a_bad_argument))["structuredContent"]
                ["error"]["code"],
            "backend_error"
        );
        assert_eq!(
            tool_error_result(&ToolError::invalid_argument(phrased_like_a_bad_argument))["structuredContent"]
                ["error"]["code"],
            "invalid_argument"
        );
    }

    #[test]
    fn the_surface_enumerates_every_error_code_it_can_return() {
        let listed = tools_list_result()["_meta"]["kmp/errorCodes"]
            .as_array()
            .expect("the codes an agent may branch on are part of the surface")
            .iter()
            .map(|entry| entry["code"].as_str().expect("a code").to_string())
            .collect::<Vec<_>>();

        for code in ToolErrorCode::ALL {
            assert!(
                listed.contains(&code.as_str().to_string()),
                "`{code}` can be returned and is not in tools/list"
            );
        }
        assert_eq!(listed.len(), ToolErrorCode::ALL.len());
    }

    #[test]
    fn jsonrpc_helpers_wrap_results_and_errors() {
        let result = serde_json::from_str::<Value>(&jsonrpc_result(json!(7), json!({"ok": true})))
            .expect("result should be JSON");
        assert_eq!(result["jsonrpc"], "2.0");
        assert_eq!(result["id"], 7);
        assert_eq!(result["result"]["ok"], true);

        let error = serde_json::from_str::<Value>(&jsonrpc_error(json!(8), -32601, "missing"))
            .expect("error should be JSON");
        assert_eq!(error["id"], 8);
        assert_eq!(error["error"]["code"], -32601);
        assert_eq!(error["error"]["message"], "missing");
    }
}
