mod json_rpc;
mod relation_vocabulary;
mod request_shape;
mod response_shape;
mod result;
mod schema;

pub(crate) use json_rpc::{jsonrpc_error, jsonrpc_result};
use relation_vocabulary::{
    relation_vocabulary_description, semantic_class_schema, writer_relation_names,
};
use request_shape::{
    budget_schema, dimensions_schema, recall_page_schema, temporal_coordinate_schema,
};
use response_shape::{
    page_output_schema, proof_output_schema, quality_output_schema, recall_envelope_properties,
    warnings_output_schema,
};
pub(crate) use result::{app_data_success_result, tool_error_result, tool_success_result};
use schema::{
    described, integer_schema, nullable_described, nullable_output_schema, output_object,
    string_array, string_map_schema, string_schema,
};

use serde_json::{Value, json};

use crate::tool_error::{ToolError, ToolErrorCode};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "underpass-kmp-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) const CHRONOLOOM_APP_URI: &str = "ui://kmp/chronoloom.html";
pub(crate) const MCP_APP_MIME: &str = "text/html;profile=mcp-app";

/// Resolves the one-minor compatibility aliases to the names advertised by
/// `tools/list`.
///
/// Alias resolution happens at the JSON-RPC boundary. Backends, telemetry and
/// newly written provenance therefore see one canonical vocabulary, while a
/// saved permission rule or script using the former names keeps working for
/// the migration release.
pub(crate) fn canonical_tool_name(name: &str) -> &str {
    match name {
        "kernel_ingest" => "kmp_ingest",
        "kernel_write_memory" => "kmp_write_memory",
        "kernel_wake" => "kmp_wake",
        "kernel_ask" => "kmp_ask",
        "kernel_goto" => "kmp_goto",
        "kernel_near" => "kmp_near",
        "kernel_rewind" => "kmp_rewind",
        "kernel_forward" => "kmp_forward",
        "kernel_trace" => "kmp_trace",
        "kernel_inspect" => "kmp_inspect",
        _ => name,
    }
}

#[cfg(test)]
pub(crate) fn initialize_result(backend: &str, grpc_tls: &str) -> Value {
    initialize_result_with_apps(backend, grpc_tls, false)
}

pub(crate) fn initialize_result_with_apps(backend: &str, grpc_tls: &str, apps: bool) -> Value {
    let mut capabilities = json!({"tools": {}});
    if apps {
        capabilities["resources"] = json!({});
        capabilities["extensions"] = json!({
            "io.modelcontextprotocol/ui": {"mimeTypes": [MCP_APP_MIME]}
        });
    }
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": capabilities,
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION
        },
        "instructions": crate::agent_policy::mcp_instructions(),
        "metadata": {
            "backend": backend,
            "grpc_tls": grpc_tls
        }
    })
}

pub(crate) fn tools_list_result() -> Value {
    tools_list_result_with_apps(false)
}

/// Canonical model-facing names declared by this protocol build. Diagnostics
/// compare the surface they observe against names, never a count that goes
/// stale as honest tools are added.
pub(crate) fn declared_tool_names() -> Vec<String> {
    tools_list_result()["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect()
}

pub(crate) fn tools_list_result_with_apps(apps: bool) -> Value {
    let mut result = tools_list_core();
    if let Some(tools) = result["tools"].as_array_mut() {
        let mut open = view_open_definition();
        if apps {
            open["_meta"] = json!({
                "ui": {
                    "resourceUri": CHRONOLOOM_APP_URI,
                    "visibility": ["model", "app"]
                }
            });
        }
        tools.push(open);
        tools.push(view_apply_intent_definition());
        tools.push(view_get_state_definition());
        if apps {
            tools.push(app_visual_projection_definition());
            tools.push(app_view_undo_definition());
        }
    }
    result
}

pub(crate) fn resources_list_result() -> Value {
    json!({
        "resources": [{
            "uri": CHRONOLOOM_APP_URI,
            "name": "KMP ChronoLoom",
            "description": "Interactive polytemporal memory loom with proof-preserving navigation.",
            "mimeType": MCP_APP_MIME,
            "_meta": {"ui": {"prefersBorder": true}}
        }]
    })
}

pub(crate) fn resource_read_result(uri: &str) -> Result<Value, ToolError> {
    if uri != CHRONOLOOM_APP_URI {
        return Err(ToolError::not_found(format!(
            "unknown MCP resource `{uri}`"
        )));
    }
    Ok(json!({
        "contents": [{
            "uri": CHRONOLOOM_APP_URI,
            "mimeType": MCP_APP_MIME,
            "text": kmp_viewer::mcp_app_html(),
            "_meta": {
                "ui": {
                    "csp": {
                        "connectDomains": [],
                        "resourceDomains": [],
                        "frameDomains": [],
                        "baseUriDomains": []
                    },
                    "prefersBorder": true
                }
            }
        }]
    }))
}

/// The memory tools. Split from the view tools below so neither `json!`
/// expansion has to hold the whole surface at once.
fn tools_list_core() -> Value {
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
                ingest_output_schema(),
            ),
            tool_definition_with_output(
                "kmp_write_memory",
                "Write to memory. This is the writer to use: it validates intent and relation quality, then commits through canonical kmp_ingest. Normal writes are one call: omit `options.dry_run` or set it to false; validation failures write nothing. Set it to true only for an explicitly requested preview or payload debugging. Reach for kmp_ingest only when producing the exact graph yourself.",
                write_memory_schema(),
                write_memory_output_schema(),
            ),
            tool_definition_with_output(
                "kmp_wake",
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
                        "depth": integer_schema("Optional graph traversal depth. Applies in embedded and live gRPC modes; it overrides budget.depth."),
                        "budget": budget_schema(1_600, 2),
                        "page": recall_page_schema()
                    }
                }),
                wake_output_schema(),
            ),
            tool_definition_with_output(
                "kmp_ask",
                "Retrieve stored entry text and evidence bearing on a question, or UNKNOWN. Nothing is generated: `answer` names what was retrieved and the text lives in `proof.evidence[].text` — `metadata.proof_role` distinguishes an entry claim from stored evidence. Read it and judge whether it answers. `proof.confidence` is lexical term overlap between the question and the best-matching memory item; it is not a judgement that the item answers, and it is not the `confidence` on a relation, which is writer certainty. UNKNOWN means memory did not answer; `summary` says whether nothing was retrieved or nothing retrieved bore on the question.",
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
                        "depth": integer_schema("Optional graph traversal depth. Applies in embedded and live gRPC modes; it overrides budget.depth."),
                        "budget": budget_schema(2_400, 2),
                        "page": recall_page_schema()
                    }
                }),
                ask_output_schema(),
            ),
            temporal_tool_definition(
                "kmp_goto",
                "Jump to memory state at a timestamp, sequence, or ref. Cursor parameter: `at`. When the result is partial, response.next_action continues earlier history with kmp_rewind; feeding page.next_cursor back to kmp_goto does not paginate.",
                "at",
            ),
            temporal_tool_definition(
                "kmp_near",
                "Return the temporal neighborhood around a timestamp, sequence, or ref. Cursor parameter: `around`. A partial neighborhood continues through response.next_action with kmp_rewind and kmp_forward; feeding page.next_cursor back to kmp_near does not paginate.",
                "around",
            ),
            temporal_tool_definition(
                "kmp_rewind",
                "Move backward through memory from a timestamp, sequence, or ref. Entries within each page are newest-to-oldest, so concatenating continuation pages stays globally descending. Cursor parameter: `from`.",
                "from",
            ),
            temporal_tool_definition(
                "kmp_forward",
                "Move forward through memory from a timestamp, sequence, or ref. Entries and continuation pages are oldest-to-newest. Cursor parameter: `from`.",
                "from",
            ),
            tool_definition_with_output(
                "kmp_trace",
                "Trace the proof path between two memory refs owned by one explicit about. Both refs are rejected before traversal unless they belong to that about.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["about", "from", "to"],
                    "properties": {
                        "about": string_schema("Memory anchor that owns both refs."),
                        "from": string_schema("Source memory ref. In live gRPC mode this must resolve to a kernel node id."),
                        "to": string_schema("Target memory ref. In live gRPC mode this must resolve to a kernel node id."),
                        "role": string_schema("Optional caller role."),
                        "goal": string_schema("Optional trace goal."),
                        "page": page_schema(),
                        "budget": budget_schema(1_600, 1)
                    }
                }),
                trace_output_schema(),
            ),
            tool_definition_with_output(
                "kmp_inspect",
                "Inspect one typed stored memory object inside an explicit about boundary. The object is stable; evidence, links and raw records page under the byte ceiling.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["about", "ref"],
                    "properties": {
                        "about": string_schema("Memory anchor that owns the inspected ref."),
                        "ref": string_schema("Memory ref to inspect. In live gRPC mode this must resolve to a kernel node id."),
                        "include": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "incoming": {
                                    "type": "boolean",
                                    "default": true,
                                    "description": "Return direct typed relations whose target is this ref. Defaults to true; set false to narrow an oversized inspection."
                                },
                                "outgoing": {
                                    "type": "boolean",
                                    "default": true,
                                    "description": "Return direct typed relations whose source is this ref. Defaults to true; set false to narrow an oversized inspection."
                                },
                                "details": {
                                    "type": "boolean",
                                    "default": true,
                                    "description": "Return the inspected object's stored details. Defaults to true."
                                },
                                "raw": {
                                    "type": "boolean",
                                    "default": false,
                                    "description": "Return typed raw audit refs for the inspected object."
                                }
                            }
                        },
                        "budget": inspect_budget_schema(),
                        "page": inspect_page_schema()
                    }
                }),
                inspect_output_schema(),
            )
        ]
    })
}

fn view_open_definition() -> Value {
    tool_definition_with_output(
        "kmp_view_open",
        "Open or rehydrate a ChronoLoom view over an about, so a human and this agent look at the same loom. Returns this session's capability link when the local viewer is mounted. Read-only with respect to memory: a view is a camera position, not a record.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["about"],
            "properties": {
                "about": string_schema("Memory anchor the loom should weave. It must exist; a view onto absent memory would render an empty loom that looks like an answer."),
                "view_id": string_schema("Which view to open. Omit for the one window a local viewer shows."),
                "expected_revision": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "When changing an existing view to another about, the view_revision this open was prepared against. A stale open conflicts instead of discarding a newer camera state."
                }
            }
        }),
        view_output_with_url_schema(),
    )
}

fn view_apply_intent_definition() -> Value {
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

fn view_get_state_definition() -> Value {
    tool_definition_with_output(
        "kmp_view_get_state",
        "Read the view's semantic state — clock, window, focus, zoom, filters, selection, revision and who last moved it. Returns this session's capability link when the local viewer is mounted; state, never pixels.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "view_id": string_schema("Which view to read. Omit for the default one.")
            }
        }),
        view_output_with_url_schema(),
    )
}

/// What every view tool answers with: the aggregate itself, its own revision,
/// and — when something was asked for that this build cannot draw — what went
/// unhonored. The list remains for future capability negotiation.
fn view_output_schema() -> Value {
    output_object(json!({
        "view_id": described("string", "The view this answer is about."),
        "view_revision": described("integer", "The view's own revision, which is not the memory's."),
        "viewer_available": described("boolean", "Whether this MCP process mounted a ChronoLoom browser for the semantic view."),
        "state": {
            "type": "object",
            "additionalProperties": true,
            "description": "The semantic view state: about, clock, focus, projection, selection, trace, search, and the provenance of the last change."
        },
        "applied": described("boolean", "Whether this call is the one that moved the view. A replayed idempotency key answers false."),
        "opened": described("boolean", "Whether a view was opened or rehydrated."),
        "unhonored": {
            "type": "array",
            "items": {"type": "string"},
            "description": "Parts of the intent this build recorded but cannot render yet."
        },
        "clocks": {"type": "array", "items": {"type": "string"}, "description": "The clocks the axis can read."},
        "semantic_zoom_ladder": {"type": "array", "items": {"type": "string"}, "description": "The rungs, coarse to fine."},
        "reads": described("string", "What this answer is, in the reader's terms."),
        "observability": described("string", "How requested overlay series are queried and aligned on the view's temporal axis.")
    }))
}

fn view_output_with_url_schema() -> Value {
    let mut schema = view_output_schema();
    schema["properties"]["url"] = described(
        "string",
        "Loopback ChronoLoom URL carrying this session's capability. Present when this MCP process mounted the local viewer.",
    );
    schema
}

fn write_memory_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["about", "intent", "actor", "observed_at", "scope", "current"],
        "properties": {
            "about": string_schema("Memory anchor or root ref this semantic memory event should attach to."),
            "intent": {
                "type": "string",
                "description": "What this write records and therefore which planner rules apply. This is a write-operation intent, not the stored entry kind: record_delta requires semantic_delta, while current.kind describes the durable fact.",
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
                "required": ["kind", "summary"],
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
                    "summary": string_schema("Concise semantic memory text to store."),
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
                        "description": "When true, only return the compiled canonical kmp_ingest preview and write nothing. Defaults to false: the call commits."
                    },
                    "strict": {
                        "type": "boolean",
                        "description": "When true, fail fast on unsupported relations and missing proof. Defaults to true."
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
            "then": {
                "properties": {
                    "current": {"required": ["kind", "summary", "evidence"]}
                }
            }
        }]
    })
}

fn read_context_schema() -> Value {
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
                "minimum": 1,
                "description": "Sequence within a temporal coordinate and dimension scope; it is not a store-global event number."
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
            "axis": {
                "type": "string",
                "enum": ["occurred", "observed", "ingested", "validity"],
                "description": "Optional clock for this read. Omit it to preserve the compatible precedence (occurred, validity start, observed, ingested). An explicit axis never substitutes another clock."
            },
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
            "depth": integer_schema("Optional graph traversal depth. Applies in embedded and live gRPC modes; it overrides budget.depth."),
            "budget": budget_schema(2_400, 3)
        }
    });
    input_schema["properties"][cursor_key] = cursor_schema;
    tool_definition_with_output(
        name,
        description,
        input_schema,
        temporal_output_schema(name, cursor_key),
    )
}

fn app_visual_projection_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["about", "from", "to"],
        "properties": {
            "about": string_schema("Memory anchor to project."),
            "from": string_schema("Inclusive RFC3339 range start."),
            "to": string_schema("Exclusive RFC3339 range end."),
            "axis": {
                "type": "string",
                "enum": ["occurred", "observed", "ingested", "validity"]
            },
            "lod": {
                "type": "string",
                "enum": ["atlas", "episode", "moment"]
            },
            "bins": {"type": "integer", "minimum": 1, "maximum": 512},
            "limit": {"type": "integer", "minimum": 1, "maximum": 2048},
            "cursor": string_schema("Opaque continuation cursor returned by the prior chunk."),
            "depth": {"type": "integer", "minimum": 1, "maximum": 6},
            "dimensions": dimensions_schema()
        }
    })
}

fn app_visual_projection_definition() -> Value {
    json!({
        "name": "kmp_view_read_projection",
        "description": "Read one bounded, paginated ChronoLoom data chunk. This tool is available to the sandboxed MCP App only; its structured payload is not a model prompt.",
        "inputSchema": app_visual_projection_schema(),
        "annotations": {
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        },
        "_meta": {
            "ui": {
                "resourceUri": CHRONOLOOM_APP_URI,
                "visibility": ["app"]
            }
        }
    })
}

fn app_view_undo_definition() -> Value {
    json!({
        "name": "kmp_view_undo",
        "description": "Undo the latest semantic view move. This is an app-only transport adapter over the same reversible view aggregate used by the loopback renderer.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "view_id": string_schema("The app view to undo. Omit for the default view.")
            }
        },
        "annotations": {
            "readOnlyHint": false,
            "destructiveHint": false,
            "idempotentHint": false,
            "openWorldHint": false
        },
        "_meta": {
            "ui": {
                "resourceUri": CHRONOLOOM_APP_URI,
                "visibility": ["app"]
            }
        }
    })
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

/// A tool, with the shape of what it answers.
///
/// Inputs were described field by field and the response — the half the agent
/// actually reasons over — was described nowhere. `proof.confidence`,
/// `proof.superseded` and `proof.expired` against `proof.conflicts`, `page.total`,
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
            "read_after_write_ready": described("boolean", "Whether a read issued now is guaranteed to observe this write.")
        })),
        "warnings": warnings_output_schema()
    }))
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

fn wake_output_schema() -> Value {
    let mut properties = json!({
        "summary": described("string", "Four-line L0 resume summary: objective, status, blocker, and next action."),
        "wake": output_object(json!({
            "objective": described("string", "The continuation intent supplied by the caller."),
            "current_state": string_array("Current semantic state selected from the rendered memory."),
            "causal_spine": described("array", "Highest-salience explanatory relations, each with claim, because, and evidence_ref."),
            "open_loops": string_array("Live blocker statements reflected by the L0 summary; empty means no blocker was identified."),
            "next_actions": string_array("Next-action statements reflected by the L0 summary; empty means no concrete next action was identified."),
            "guardrails": string_array("Stored constraints the continuation should preserve.")
        })),
        "proof": proof_output_schema("Wake uses medium for a non-empty deterministic retrieval packet; this is not relation-writer certainty."),
        "resume_cursor": nullable_described("object", "Newest temporal coordinate covered by this packet; null when the packet carries no temporal anchor.")
    });
    properties
        .as_object_mut()
        .expect("output properties")
        .extend(
            recall_envelope_properties()
                .as_object()
                .expect("envelope properties")
                .clone(),
        );
    output_object(properties)
}

fn ask_output_schema() -> Value {
    let mut properties = json!({
        "summary": described("string", "States whether nothing was retrieved, retrieved evidence did not bear on the question, or evidence was retained."),
        "answer": nullable_described("string", "UNKNOWN when the selected policy found no answerable evidence; otherwise names the retrieved citations without claiming they prove the answer."),
        "because": described("array", "At most five retained citation refs. Empty beside UNKNOWN; canonical text lives in proof.evidence."),
        "proof": proof_output_schema("Derived from lexical term overlap between the question and the best retained evidence item. It is not a judgement that the evidence answers the question, and it is not relation-writer certainty.")
    });
    properties
        .as_object_mut()
        .expect("output properties")
        .extend(
            recall_envelope_properties()
                .as_object()
                .expect("envelope properties")
                .clone(),
        );
    output_object(properties)
}

fn temporal_output_schema(tool_name: &str, cursor_key: &str) -> Value {
    let cursor_description = match tool_name {
        "kmp_goto" => "Boundary ref for earlier history. Do not pass it back to `at.ref`; follow top-level `next_action`, which continues with `kmp_rewind`.".to_string(),
        "kmp_near" => "Boundary ref for a partial neighborhood. Do not pass it back to `around.ref`; follow top-level `next_action`, which names the earlier and later moves.".to_string(),
        _ => format!("Memory-ref cursor for the next temporal slice; place it in `{cursor_key}.ref` while keeping the other arguments unchanged."),
    };
    output_object(json!({
        "summary": described("string", "Concise description of the temporal selection."),
        "next_action": nullable_described("string", "The exact temporal move that can consume a partial result's boundary cursor, or null when the selection is complete."),
        "temporal": nullable_described("object", "Resolved direction, selected clock axis, requested cursor, and resolved coordinate."),
        "coverage": output_object(json!({
            "requested": nullable_described("object", "Dimension selection requested by the caller."),
            "included": string_array("Dimension scope ids included in the result."),
            "missing": string_array("Requested dimension scope ids not present in the result."),
            "dimensions": described("array", "Per-dimension returned counts and presence flags.")
        })),
        "entries": described("array", "Temporal entries in traversal order, each with ref, kind, text, coordinates, and metadata."),
        "page": page_output_schema("temporal entries", &cursor_description),
        "raw_refs": described("array", "Typed raw audit refs for selected entries when include.raw_refs=true."),
        "proof": proof_output_schema("Temporal reads use medium when entries were returned and unknown when none were returned; this is not relation-writer certainty."),
        "quality": nullable_output_schema(quality_output_schema(), "Response-shape metrics; null when the backend supplied none."),
        "warnings": warnings_output_schema()
    }))
}

fn trace_output_schema() -> Value {
    output_object(json!({
        "summary": described("string", "Concise statement of the path selection."),
        "trace": described("array", "Ordered typed relations connecting from to to; empty means no path in the same memory graph."),
        "page": page_output_schema("trace relations", "Opaque trace cursor; repeat it as page.cursor with every other argument unchanged."),
        "quality": nullable_output_schema(quality_output_schema(), "Response-shape metrics; null when the backend supplied none."),
        "warnings": warnings_output_schema()
    }))
}

fn inspect_output_schema() -> Value {
    output_object(json!({
        "summary": described("string", "Concise statement of which typed object was inspected."),
        "object": described("object", "Stored object with ref, kind, canonical text, metadata, and optional source."),
        "links": output_object(json!({
            "incoming": described("array", "Direct typed relations whose target is the inspected ref."),
            "outgoing": described("array", "Direct typed relations whose source is the inspected ref.")
        })),
        "evidence": described("array", "Direct stored evidence for the object; text is canonical and supports names the refs it anchors."),
        "raw": described("array", "Typed raw audit records returned only when include.raw=true."),
        "page": output_object(json!({
            "offset": described("integer", "Number of expansion items reconstructed by earlier pages."),
            "returned": described("integer", "Number of expansion items returned on this page."),
            "total": described("integer", "Total evidence, outgoing-link, incoming-link and raw expansion items in the selection."),
            "has_more": described("boolean", "Whether expansion items remain after this page."),
            "next_cursor": nullable_described("string", "Opaque inspect cursor. Repeat the same bound arguments with this value in page.cursor."),
            "omitted": described("object", "Counts still remaining after this page, by details, evidence, outgoing, incoming and raw section."),
            "sections": described("object", "Per-section returned-on-page, remaining and total counts."),
            "required_bytes": described("integer", "Exact serialized bytes required by the complete inspection, so a partial result never forces callers to probe budgets."),
            "guidance": nullable_described("string", "Continuation, narrowing and budget guidance when this response is partial; null for a complete first page.")
        })),
        "quality": nullable_output_schema(quality_output_schema(), "Response-shape metrics; null when the backend supplied none."),
        "warnings": warnings_output_schema()
    }))
}

fn inspect_budget_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "max_bytes": {
                "type": "integer",
                "minimum": 512,
                "default": 10_000,
                "description": "Normative maximum bytes for structuredContent. Inspect pages expandable sections instead of overflowing the host and errors only when the stable object itself cannot fit."
            }
        }
    })
}

fn inspect_page_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "cursor": {
                "type": "string",
                "minLength": 1,
                "description": "Opaque cursor returned by inspect page.next_cursor. Repeat all bound arguments unchanged; budget.max_bytes may change."
            }
        }
    })
}

/// Applies the strictness the schemas already declare.
///
/// Every model-facing tool says `"additionalProperties": false` and nothing
/// enforced it, which made the surface a silent-failure generator: a
/// misspelled `dimensions`, a `budget` nested one level too deep, a `from`
/// sent to `kmp_goto` where the cursor is `at` — each accepted, dropped,
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
/// full tool document — relation vocabulary included — from scratch each time.
/// Rebuilding a document that cannot change, per call, to read one field of
/// it, is a cost with nothing on the other side of it.
fn tool_input_schema(tool: &str) -> Option<&'static Value> {
    if tool == "kmp_view_read_projection" {
        static APP_VISUAL_SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        return Some(APP_VISUAL_SCHEMA.get_or_init(app_visual_projection_schema));
    }
    if tool == "kmp_view_undo" {
        static APP_UNDO_SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        return Some(
            APP_UNDO_SCHEMA.get_or_init(|| app_view_undo_definition()["inputSchema"].clone()),
        );
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    use kmp_domain::KnownMemoryRelationType;

    #[test]
    fn former_tool_names_resolve_to_the_advertised_kmp_surface() {
        let former = [
            "kernel_ingest",
            "kernel_write_memory",
            "kernel_wake",
            "kernel_ask",
            "kernel_goto",
            "kernel_near",
            "kernel_rewind",
            "kernel_forward",
            "kernel_trace",
            "kernel_inspect",
        ];
        let tools = tools_list_result();
        let current = tools["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .collect::<Vec<_>>();

        // Every former name still resolves to something this surface
        // advertises. It is not an equality: the view tools were born with
        // their kmp_ names and never had a kernel_ one to rename.
        for name in former.map(canonical_tool_name) {
            assert!(
                current.contains(&name),
                "former name resolves to `{name}`, which the surface no longer advertises"
            );
        }
        assert!(current.iter().all(|name| name.starts_with("kmp_")));
    }

    /// The view half is small on purpose, and every one of its moves is
    /// under concurrency control: an agent that cannot be told "the human
    /// moved first" would eventually yank the loom out from under someone.
    #[test]
    fn the_view_tools_are_declarative_idempotent_and_conflict_aware() {
        let result = tools_list_result();
        let tools = result["tools"].as_array().expect("tools");
        let view = |name: &str| {
            tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("`{name}` is not advertised"))
                .clone()
        };

        let intent = view("kmp_view_apply_intent");
        assert_eq!(intent["inputSchema"]["required"][0], "idempotency_key");
        let properties = &intent["inputSchema"]["properties"];
        assert!(properties.get("expected_revision").is_some());
        assert!(properties.get("explanation").is_some());
        // Semantic vocabulary only — no coordinates, no pixels, no code.
        for forbidden in ["x", "y", "zoom_level", "camera", "html", "script"] {
            assert!(
                properties.get(forbidden).is_none(),
                "`{forbidden}` would make the agent drive pixels instead of meaning"
            );
        }
        let axis = &properties["focus"]["properties"]["time_range"]["properties"]["axis"]["enum"];
        assert_eq!(axis[0], "occurred", "the clock is chosen, never assumed");
        assert_eq!(
            properties["projection"]["properties"]["semantic_zoom"]["enum"],
            json!(["atlas", "episode", "moment"]),
            "evidence is opened by selecting an entry, not requested as a zoom rung"
        );

        assert_eq!(view("kmp_view_open")["inputSchema"]["required"][0], "about");
        for name in ["kmp_view_open", "kmp_view_get_state"] {
            assert!(
                view(name)["outputSchema"]["properties"]["url"]["description"]
                    .as_str()
                    .expect("viewer URL description")
                    .contains("capability"),
                "{name} must advertise the handoff link"
            );
        }
        assert!(
            view("kmp_view_apply_intent")["outputSchema"]["properties"]
                .get("url")
                .is_none(),
            "only discovery tools promise the capability handoff"
        );
        assert!(
            view("kmp_view_get_state")["description"]
                .as_str()
                .expect("description")
                .contains("never pixels")
        );
    }

    #[test]
    fn initialize_result_reports_backend_metadata() {
        let result = initialize_result("stub", "mutual");

        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], SERVER_NAME);
        assert_eq!(result["metadata"]["backend"], "stub");
        assert_eq!(result["metadata"]["grpc_tls"], "mutual");
        let instructions = result["instructions"].as_str().expect("instructions");
        assert!(instructions.contains("Temporal intent has precedence"));
        assert!(instructions.contains("Preserve evidence text"));
        assert!(instructions.contains("Refs are opaque identifiers"));
        assert!(instructions.contains("Never prefix or qualify it with an about"));
    }

    #[test]
    fn tools_list_result_exposes_expected_tool_shapes() {
        let result = tools_list_result();
        let tools = result["tools"]
            .as_array()
            .expect("tools should be an array");

        assert_eq!(tools.len(), 13, "ten memory tools and three view tools");
        assert_eq!(tools[0]["name"], "kmp_ingest");
        assert_eq!(tools[0]["inputSchema"]["required"][1], "memory");
        assert_eq!(tools[1]["name"], "kmp_write_memory");
        let writer_description = tools[1]["description"]
            .as_str()
            .expect("writer description");
        assert!(writer_description.contains("Normal writes are one call"));
        assert!(writer_description.contains("validation failures write nothing"));
        assert!(writer_description.contains("explicitly requested preview"));
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
        assert_eq!(tools[2]["name"], "kmp_wake");
        assert_eq!(tools[2]["inputSchema"]["required"][0], "about");
        assert_eq!(tools[2]["_meta"]["anthropic/maxResultSizeChars"], 10_000);
        assert_eq!(
            tools[2]["inputSchema"]["properties"]["budget"]["properties"]["max_bytes"]["minimum"],
            512
        );
        assert!(tools[2]["inputSchema"]["properties"].get("page").is_some());
        assert_eq!(tools[3]["name"], "kmp_ask");
        assert_eq!(tools[3]["inputSchema"]["required"][1], "question");
        assert_eq!(tools[3]["_meta"]["anthropic/maxResultSizeChars"], 10_000);
        assert!(tools[3]["inputSchema"]["properties"].get("page").is_some());
        assert!(
            tools[3]["inputSchema"]["properties"]
                .get("prefer")
                .is_none()
        );
        assert_eq!(tools[4]["name"], "kmp_goto");
        assert_eq!(tools[4]["inputSchema"]["required"][1], "at");
        assert!(tools[4]["description"].as_str().is_some_and(|description| {
            description.contains("feeding page.next_cursor back to kmp_goto does not paginate")
        }));
        assert!(
            tools[4]["outputSchema"]["properties"]
                .get("next_action")
                .is_some()
        );
        assert!(
            tools[4]["outputSchema"]["properties"]["page"]["properties"]["next_cursor"]
                ["description"]
                .as_str()
                .is_some_and(|description| description.contains("Do not pass it back to `at.ref`"))
        );
        assert!(
            tools[5]["outputSchema"]["properties"]["page"]["properties"]["next_cursor"]
                ["description"]
                .as_str()
                .is_some_and(|description| description.contains("Do not pass it back to `around.ref`"))
        );
    }

    #[test]
    fn semantic_input_fields_have_a_typed_grpc_or_explicit_helper_classification() {
        fn keys(value: &Value) -> std::collections::BTreeSet<String> {
            value
                .get("properties")
                .and_then(Value::as_object)
                .expect("schema properties")
                .keys()
                .cloned()
                .collect()
        }

        fn expected(values: &[&str]) -> std::collections::BTreeSet<String> {
            values.iter().map(|value| (*value).to_string()).collect()
        }

        let contract = tools_list_result();
        let tools = contract["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|tool| (tool["name"].as_str().expect("name"), tool))
            .collect::<std::collections::BTreeMap<_, _>>();
        let schema = |name: &str| &tools[name]["inputSchema"];

        assert_eq!(
            keys(schema("kmp_ingest")),
            expected(&[
                "about",
                "dry_run",
                "idempotency_key",
                "memory",
                "provenance"
            ])
        );
        assert_eq!(
            keys(schema("kmp_wake")),
            expected(&[
                "about",
                "budget",
                "depth",
                "dimensions",
                "intent",
                "page",
                "role"
            ])
        );
        assert_eq!(
            keys(schema("kmp_ask")),
            expected(&[
                "about",
                "answer_policy",
                "budget",
                "depth",
                "dimensions",
                "page",
                "question",
            ])
        );
        for (name, cursor) in [
            ("kmp_goto", "at"),
            ("kmp_near", "around"),
            ("kmp_rewind", "from"),
            ("kmp_forward", "from"),
        ] {
            assert_eq!(
                keys(schema(name)),
                expected(&[
                    "about",
                    "axis",
                    "budget",
                    "depth",
                    "dimensions",
                    "include",
                    "limit",
                    "window",
                    cursor,
                ]),
                "{name} typed request crosswalk"
            );
            assert_eq!(
                keys(&schema(name)["properties"][cursor]),
                expected(&["ref", "sequence", "time"])
            );
        }
        assert_eq!(
            keys(schema("kmp_trace")),
            expected(&["about", "budget", "from", "goal", "page", "role", "to"])
        );
        assert_eq!(
            keys(schema("kmp_inspect")),
            expected(&["about", "budget", "include", "page", "ref"])
        );

        // These shared shapes are one-to-one with MemoryBudget,
        // DimensionSelection and PageRequest. `depth` at the tool root is an
        // explicit compatibility alias for MemoryBudget.depth; Trace.role is
        // an alias for TraceRequest.goal. Inspect.budget and Inspect.page are
        // transport-only result projection and never cross the gRPC request;
        // its budget may only carry max_bytes.
        let wake_properties = &schema("kmp_wake")["properties"];
        assert_eq!(
            keys(&wake_properties["budget"]),
            expected(&["depth", "detail", "max_bytes", "max_entries", "tokens"])
        );
        assert_eq!(
            keys(&wake_properties["dimensions"]),
            expected(&["abouts", "exclude", "include", "mode", "scope", "scope_ids"])
        );
        assert_eq!(
            keys(&wake_properties["page"]),
            expected(&["cursor", "entries"])
        );
        assert_eq!(
            keys(&schema("kmp_inspect")["properties"]["budget"]),
            expected(&["max_bytes"])
        );
        assert_eq!(
            keys(&schema("kmp_inspect")["properties"]["page"]),
            expected(&["cursor"])
        );

        // The writer is the sole non-RPC tool: every field belongs to the
        // validated helper contract and its output is pinned to canonical
        // Ingest by the writer compilation and four-path parity tests.
        assert_eq!(
            keys(schema("kmp_write_memory")),
            expected(&[
                "about",
                "actor",
                "connect_to",
                "current",
                "idempotency_key",
                "intent",
                "occurred_at",
                "observed_at",
                "options",
                "rank",
                "read_context",
                "scope",
                "semantic_delta",
                "source_kind",
                "valid_from",
                "valid_until",
            ])
        );
    }

    #[test]
    fn every_tool_describes_the_response_it_returns() {
        let tools = tools_list_result();
        let tools = tools["tools"].as_array().expect("tools");

        for tool in tools {
            let name = tool["name"].as_str().expect("tool name");
            let schema = &tool["outputSchema"];
            assert_eq!(schema["type"], "object", "{name} output is an object");
            assert_eq!(
                schema["additionalProperties"], false,
                "{name} must not grow an unexplained response field"
            );
            let properties = schema["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("{name} output properties"));
            assert!(!properties.is_empty(), "{name} describes its response");
            for (field, field_schema) in properties {
                let explained = field_schema
                    .get("description")
                    .and_then(Value::as_str)
                    .is_some_and(|description| !description.trim().is_empty())
                    || field_schema
                        .get("properties")
                        .and_then(Value::as_object)
                        .is_some_and(|nested| !nested.is_empty());
                assert!(explained, "{name}.outputSchema.{field} has a meaning");
            }
        }
    }

    #[test]
    fn writer_schema_allows_the_relation_free_root_that_runtime_accepts() {
        let contract = tools_list_result();
        let writer = contract["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .find(|tool| tool["name"] == "kmp_write_memory")
            .map(|tool| &tool["inputSchema"])
            .expect("writer schema");
        let required = writer["required"]
            .as_array()
            .expect("writer required fields");
        assert!(
            !required.iter().any(|field| field == "connect_to"),
            "a new about has no existing ref to connect its first memory to"
        );
        assert!(
            writer["properties"]["connect_to"].get("minItems").is_none(),
            "an explicit empty connect_to must describe the same valid root write"
        );
        assert!(
            writer["properties"]["connect_to"]["description"]
                .as_str()
                .is_some_and(|description| description.contains("first strict write")),
            "tools/list must explain the data-dependent runtime rule"
        );
    }

    #[test]
    fn output_schemas_cover_the_mapper_top_level_fields() {
        use kmp_proto::v1beta1::{
            AskResponse, IngestResponse, InspectResponse, TemporalMoveResponse, TraceResponse,
            WakeResponse,
        };

        let tools = tools_list_result();
        let schemas = tools["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|tool| (tool["name"].as_str().expect("name"), &tool["outputSchema"]))
            .collect::<std::collections::BTreeMap<_, _>>();
        let recall_arguments = json!({});
        let samples = [
            (
                "kmp_ingest",
                crate::kmp::ingest_from_response(IngestResponse::default()),
            ),
            (
                "kmp_wake",
                crate::kmp::enforce_recall_output_budget(
                    crate::kmp::wake_from_response(WakeResponse::default()),
                    &recall_arguments,
                    1_600,
                ),
            ),
            (
                "kmp_ask",
                crate::kmp::enforce_recall_output_budget(
                    crate::kmp::ask_from_response(AskResponse::default()),
                    &recall_arguments,
                    2_400,
                ),
            ),
            (
                "kmp_goto",
                crate::kmp::temporal_from_response(TemporalMoveResponse::default()),
            ),
            (
                "kmp_trace",
                crate::kmp::trace_from_response(TraceResponse::default()),
            ),
            (
                "kmp_inspect",
                crate::kmp::inspect_from_response(InspectResponse::default()),
            ),
        ];

        for (name, sample) in samples {
            let described = schemas[name]["properties"]
                .as_object()
                .expect("schema properties");
            for field in sample.as_object().expect("mapped response").keys() {
                assert!(
                    described.contains_key(field),
                    "{name} returns `{field}` but its outputSchema does not describe it"
                );
            }
        }

        for name in ["kmp_near", "kmp_rewind", "kmp_forward"] {
            assert_eq!(
                schemas[name]["properties"]
                    .as_object()
                    .expect("temporal properties")
                    .keys()
                    .collect::<Vec<_>>(),
                schemas["kmp_goto"]["properties"]
                    .as_object()
                    .expect("temporal properties")
                    .keys()
                    .collect::<Vec<_>>()
            );
        }
        for field in [
            "accepted",
            "dry_run",
            "summary",
            "generated_refs",
            "relations",
            "relation_quality",
            "relation_quality_metrics",
            "diagnostics",
            "next_suggested_reads",
        ] {
            assert!(
                schemas["kmp_write_memory"]["properties"]
                    .get(field)
                    .is_some(),
                "writer output describes `{field}`"
            );
        }
    }

    #[test]
    fn each_verb_publishes_the_defaults_its_backend_uses() {
        let tools = tools_list_result();
        let tools = tools["tools"].as_array().expect("tools");
        let defaults = [
            ("kmp_wake", 1_600, 2),
            ("kmp_ask", 2_400, 2),
            ("kmp_goto", 2_400, 3),
            ("kmp_near", 2_400, 3),
            ("kmp_rewind", 2_400, 3),
            ("kmp_forward", 2_400, 3),
            ("kmp_trace", 1_600, 1),
        ];

        for (name, tokens, depth) in defaults {
            let tool = tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("{name}"));
            let budget = &tool["inputSchema"]["properties"]["budget"]["properties"];
            assert_eq!(budget["tokens"]["default"], tokens, "{name} tokens");
            assert_eq!(budget["depth"]["default"], depth, "{name} depth");
            assert_eq!(budget["max_bytes"]["default"], 10_000);
            assert_eq!(budget["detail"]["default"], "balanced");
        }
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
