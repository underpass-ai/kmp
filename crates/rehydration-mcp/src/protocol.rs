use rehydration_domain::KnownMemoryRelationType;
use serde_json::{Value, json};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "rehydration-kernel-kmp";
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
                                            "rel": string_schema("Relationship type."),
                                            "class": {
                                                "type": "string",
                                                "enum": ["structural", "causal", "motivational", "procedural", "evidential", "constraint"]
                                            },
                                            "why": string_schema("Optional relation rationale; required for non-structural relations unless evidence is set."),
                                            "evidence": string_schema("Optional relation evidence; required for non-structural relations unless why is set."),
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
                                "observed_at": string_schema("RFC3339 observation timestamp."),
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
                "Plan or commit a writer-friendly semantic memory event. The tool validates writer intent, relation quality, and compiles to canonical kernel_ingest.",
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
                        "budget": budget_schema()
                    }
                })
            ),
            tool_definition(
                "kernel_ask",
                "Return a deterministic evidence answer from kernel memory, or UNKNOWN.",
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
                        "budget": budget_schema()
                    }
                })
            ),
            temporal_tool_definition(
                "kernel_goto",
                "Jump to memory state at a timestamp, sequence, or ref over selected dimensions.",
                "at",
            ),
            temporal_tool_definition(
                "kernel_near",
                "Return the temporal neighborhood around a timestamp, sequence, or ref.",
                "around",
            ),
            temporal_tool_definition(
                "kernel_rewind",
                "Move backward through memory from a timestamp, sequence, or ref.",
                "from",
            ),
            temporal_tool_definition(
                "kernel_forward",
                "Move forward through memory from a timestamp, sequence, or ref.",
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
            "observed_at": string_schema("RFC3339 timestamp for provenance and default coordinates."),
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
                            "enum": writer_relation_names()
                        },
                        "class": {
                            "type": "string",
                            "enum": ["structural", "causal", "motivational", "procedural", "evidential", "constraint"]
                        },
                        "why": string_schema("Why this relation exists. Required for non-structural relations."),
                        "evidence": string_schema("Evidence for this relation. Required for non-structural relations."),
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
                        "description": "When true, only return the compiled canonical kernel_ingest preview."
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
                "enum": ["current_about", "abouts", "all_abouts"]
            },
            "abouts": {
                "type": "array",
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
            "observed_at": string_schema("Optional RFC3339 observation timestamp."),
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
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
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

fn budget_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "tokens": {
                "type": "integer",
                "minimum": 1
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

pub(crate) fn tool_success_result(structured_content: Value) -> Value {
    let text = serde_json::to_string_pretty(&structured_content)
        .expect("fixture JSON should serialize as pretty text");
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

pub(crate) fn tool_error_result(message: &str) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": message
            }
        ],
        "structuredContent": {
            "error": {
                "code": tool_error_code(message),
                "message": message
            }
        },
        "isError": true
    })
}

fn tool_error_code(message: &str) -> &'static str {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("not found") {
        "not_found"
    } else if normalized.contains("unknown kmp tool") {
        "unknown_tool"
    } else if normalized.contains("failed to connect") || normalized.contains("unavailable") {
        "unavailable"
    } else if normalized.contains("missing")
        || normalized.contains("required")
        || normalized.contains("invalid")
        || normalized.contains("must")
    {
        "invalid_argument"
    } else {
        "backend_error"
    }
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
        assert_eq!(tools[2]["name"], "kernel_wake");
        assert_eq!(tools[2]["inputSchema"]["required"][0], "about");
        assert_eq!(tools[3]["name"], "kernel_ask");
        assert_eq!(tools[3]["inputSchema"]["required"][1], "question");
        assert!(
            tools[3]["inputSchema"]["properties"]
                .get("prefer")
                .is_none()
        );
        assert_eq!(tools[4]["name"], "kernel_goto");
        assert_eq!(tools[4]["inputSchema"]["required"][1], "at");
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

        let error = tool_error_result("no evidence");
        assert_eq!(error["isError"], true);
        assert_eq!(error["content"][0]["text"], "no evidence");
        assert_eq!(error["structuredContent"]["error"]["code"], "backend_error");

        let missing = tool_error_result("node `question:missing` not found");
        assert_eq!(missing["structuredContent"]["error"]["code"], "not_found");
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
