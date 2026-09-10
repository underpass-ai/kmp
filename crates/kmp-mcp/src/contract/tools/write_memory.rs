use crate::contract::writer_memory_kinds::WRITER_MEMORY_KINDS;
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
        "Write memory with evidence. Use memories for one or more records with local ids, labels and justified links, validated as one packet before canonical ingest. Normal writes are one call: omit `options.dry_run` or set it to false; validation failures write nothing. Set it to true only for an explicitly requested preview or payload debugging. Reach for kmp_ingest only when producing the exact graph yourself.",
        write_memory_schema(),
        write_memory_output_schema(),
    )
}

pub(crate) fn write_memory_schema() -> Value {
    let labels = json!({
        "type": "object",
        "additionalProperties": {"type":"array","minItems":1,"uniqueItems":true,"items":{"type":"string","minLength":1}},
        "description": "Key-to-array memberships. Every declared value is materialized. Sharing a string does not prove entity identity; reuse the about catalogue's intended vocabulary."
    });
    let observed = string_schema(
        "RFC3339 observation time, with its true UTC offset. More than five minutes ahead of the kernel clock is refused; backfill is allowed. Actual ingestion is recorded separately.",
    );
    let occurred = string_schema(
        "When the event occurred, if known. Omission remains unknown; observation is not a substitute.",
    );
    let valid_from = string_schema(
        "Inclusive start of the recorded state's validity, only if known. Omit an unknown start; observation time is not a substitute.",
    );
    let valid_until = string_schema(
        "Exclusive end of the recorded state's validity, only if known from this source. Do not backdate knowledge from a later report.",
    );
    let rank = json!({"type":"integer","minimum":1});
    let relation = json!({
        "type":"object","additionalProperties":false,"required":["ref","rel"],
        "if":{"required":["rel"],"properties":{"rel":{"enum":writer_relations_requiring_class()}}},
        "then":{"required":["class"]},
        "properties":{
            "ref":string_schema("Exact local id (with or without @) in this packet, or an existing canonical ref. No fuzzy matching. Stored rich targets require read_context. Only same_event_as/same_entity_as may cross abouts, with the returned kmp_relate proposal."),
            "rel":{"type":"string","enum":writer_relation_names(),"description":relation_vocabulary_description("Choose the specific relation justified by the source.")},
            "class":writer_class_schema(),
            "why":string_schema("Why this specific semantic connection holds and what a later reader should understand. Required for non-structural links."),
            "evidence":string_schema("The concrete observation or source supporting the relation rationale. Required for non-structural links."),
            "confidence":{"type":"string","enum":["high","medium","low","unknown"]}
        }
    });
    let memories = json!({
        "type":"array","minItems":1,
        "description":"One or more source-backed records. All ids and proof links are validated before one commit in this about. Shared labels union with record labels; every record needs at least one membership. A one-record packet uses the same shape. Independent facts may be unlinked; do not invent relations.",
        "items":{
            "type":"object","additionalProperties":false,
            "required":["id","kind","summary"],
            "properties":{
                "id":{"type":"string","pattern":"^[A-Za-z][A-Za-z0-9_-]*$","description":"Local name. Use this exact name or @name in connect_to.ref, including forward links. local_refs returns its canonical address."},
                "ref":string_schema("Omit for a new memory. An explicit canonical ref updates that exact entry and must be a safe descendant of this about, not its anchor or an internal evidence/dimension object."),
                "kind":{"type":"string","enum":WRITER_MEMORY_KINDS,"description":"What this memory records. No separate writer intent is needed."},
                "summary":string_schema("Literal memory text, in the language of the work. Ask cites this text byte for byte."),
                "summary_en":string_schema("Your English search rendering, retaining numbers, identifiers and acronyms. Strict mode requires it for non-English summary and rejects a wrong-language, thin, identical or identifier-dropping rendering. Search uses this field; citations retain summary. Consult Write for examples."),
                "evidence":string_schema("Concrete source or observation supporting this memory. Required unless options.strict is explicitly false."),
                "labels":labels,
                "observed_at":observed,
                "occurred_at":occurred,
                "valid_from":valid_from,
                "valid_until":valid_until,
                "rank":rank,
                "connect_to":{
                    "description":"Justified links: this containing memory is the source, connect_to.ref is the target. Read each as source -> rel -> target before submitting.",
                    "type":"array","items":relation
                }
            }
        }
    });
    json!({
        "type":"object", "additionalProperties":false,
        "required":["about","actor","observed_at"],
        "properties":{
            "about":string_schema("Exact about receiving this one transaction. Never inferred or changed by a default."),
            "actor":string_schema("Human, agent or component producing the write."),
            "observed_at":string_schema("Required packet provenance and shared observation time, even if every record overrides it. Supply the actual RFC3339 observation with its UTC offset; not an assumed occurrence or validity start. Backfill allowed; more than five minutes ahead of the kernel clock is refused."),
            "occurred_at":occurred,
            "valid_from":valid_from,
            "valid_until":valid_until,
            "rank":rank,
            "source_kind":{"type":"string","enum":["human","agent","projection","derived"]},
            "labels":labels,
            "memories":memories,
            "search_summaries":{
                "type":"array","minItems":1,
                "description":"Attach English search renderings to existing memories, separately from memories. KMP reads the stored text, kind, coordinates and metadata first and preserves them. Duplicate targets or an invalid rendering reject the whole packet.",
                "items":{"type":"object","additionalProperties":false,"required":["ref","summary_en"],"properties":{
                    "ref":string_schema("Existing memory in this about. The stored source is not replaced."),
                    "summary_en":string_schema("English search rendering of the stored source, retaining its numbers, identifiers and acronyms.")
                }}
            },
            "read_context":read_context_schema(),
            "idempotency_key":string_schema("One stable key per logical packet. Exact retries keep refs; a different payload must not reuse an accepted key. Omit to derive the key from the payload."),
            "options":{
                "type":"object","additionalProperties":false,
                "properties":{
                    "dry_run":{"type":"boolean","description":"Explicit preview against the selected store, without commit or reservation. Defaults to false: ordinary writes validate and commit in one call. Requires an available backend."},
                    "strict":{"type":"boolean","description":"Defaults to true. Requires evidence, justified supported relations, prior context for stored rich targets, and valid search renderings; refuses resembling new labels unless confirmed."},
                    "labels_new":{"type":"array","items":{"type":"string"},"description":"Keys whose new values are intentional after consulting the catalogue. Every named key must occur in the packet; the kernel neither renames nor merges a label silently."},
                    "sequence":{"type":"integer","minimum":1,"description":"Optional first sequence, advanced per record. Omit for the next free sequence in each coordinate. Not used for search_summaries, which preserve stored coordinates."}
                }
            }
        },
        "if":{"required":["memories"]},
        "then":{
            "not":{"required":["search_summaries"]},
            "if":{"not":{"required":["options"],"properties":{"options":{"required":["strict"],"properties":{"strict":{"const":false}}}}}},
            "then":{"properties":{"memories":{"items":{"required":["evidence"]}}}}
        },
        "else":{
            "required":["search_summaries"],
            "properties":{"options":{"not":{"anyOf":[{"required":["sequence"]},{"required":["labels_new"]}]}}},
            "not":{"anyOf":[{"required":["labels"]},{"required":["occurred_at"]},{"required":["valid_from"]},{"required":["valid_until"]},{"required":["rank"]},{"required":["read_context"]}]}
        }
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
        "feedback": json!({
            "type": "array",
            "description": "Validation signals. Codes and paths come from validation, not message parsing. A warning can accompany acceptance; never invent missing proof.",
            "items": output_object(json!({
                "code": described("string", "Stable rule identifier."),
                "severity": described("string", "error for a refusal; warning for accepted relations whose prior context needs review."),
                "field": described("string", "Argument path, for example memories[1].evidence; empty denotes the request as a whole."),
                "reason": described("string", "What must be corrected using actual sources."),
                "allowed_values": string_array("Valid vocabulary when the refused field has one; choose using the source, never automatically substitute a kind."),
                "expected_type": described("string", "JSON type required by an INVALID_TYPE refusal, such as array or object."),
                "received_type": described("string", "Actual JSON type at the refused field; a JSON-encoded string remains string and is never coerced."),
                "action": {"anyOf": [output_object(json!({
                    "tool": described("string", "Supported reading move, not proof that the proposed relation holds."),
                    "arguments": described("object", "Complete arguments to execute that read.")
                })), {"type":"null"}]}
            }))
        }),
        "accepted": described("boolean", "True only when the canonical ingest was committed; false for a dry-run preview."),
        "status": json!({"type":"string","enum":["committed","replayed","validated","rejected","unconfirmed"],"description":"committed appended a command; replayed returned its earlier acceptance; validated is a preview; rejected failed validation. unconfirmed cannot establish persistence: retain the logical key when resolving a transport failure."}),
        "clocks": crate::contract::schema::write_clocks::write_clocks_schema(),
        "dry_run": described("boolean", "Whether this response is a validated preview that wrote nothing."),
        "validation": output_object(json!({
            "scope": described("string", "current_store for a live embedded/gRPC preview; fixture for a simulated backend. A successful preview is not a reservation or a commit.")
        })),
        "warnings": string_array("Store notices qualifying this preview or accepted write; retain them."),
        "read_after_write_ready": described("boolean", "Whether a read now observes the accepted write."),
        "coverage": output_object(json!({
            "scope": described("string", "submitted_packet: coverage of the declaration only."),
            "complete": described("boolean", "All declared records and label memberships validated; accepted determines persistence."),
            "source_coverage": described("string", "not_assessed: KMP cannot detect facts omitted by the writer."),
            "memories": described("integer", "Declared source memories."),
            "search_summaries": described("integer", "Updated search renderings."),
            "relations": described("integer", "Declared semantic links."),
            "evidence": described("integer", "Compiled evidence objects."),
            "label_memberships": described("integer", "Declared per-memory key/value pairs after shared-label union."),
            "preserved_memberships": described("integer", "Existing memberships preserved by search_summaries.")
        })),
        "receipt": output_object(json!({
            "ref": described("string", "Immutable accepted-command audit ref. Absent for previews and simulated backends."),
            "action": output_object(json!({
                "tool": described("string", "kmp_inspect; execute only when accepted detail is needed."),
                "arguments": described("object", "Complete arguments. object.text holds JSON with receipt.canonical_memory, receipt.writer diagnostics and command revision/hash. It is a historical snapshot, not current graph state.")
            }))
        })),
        "summary": described("string", "Counts and scope of the semantic write the planner prepared."),
        "generated_refs": string_array("Stable refs generated for entries whose ref the caller omitted. Their identity suffix is deterministic for an exact logical-write retry and distinct across different writes."),
        "local_refs": json!({"type": "object", "additionalProperties": {"type": "string"}, "description": "For a memories packet, local id to canonical memory ref. Preview refs are planned; accepted=true confirms persistence."}),
        "labels": output_object(json!({
            "written": described("array", "The labels this write carries, each `key` and `value`, in the order their coordinates were emitted."),
            "created": described("array", "Of those, the labels the about did not hold before this write. Present only after a committed write; vocabulary grows here, so read it."),
            "resembling": described("array", "After a committed non-strict write: the labels written that resemble one the about already held, each with `key`, `value`, `existing_key`, `existing_value`, `kind` and `why`. Under strict such a write is refused instead, naming both labels, unless `options.labels_new` insists.")
        })),
        "relations": json!({
            "type":"array",
            "description":"Compiled source -> relation -> target triples, in canonical ingest order. @id resolves through local_refs; other endpoints remain canonical refs. Review direction and scope against the source: acceptance does not prove semantic fidelity.",
            "items":{
                "type":"object","additionalProperties":false,"required":["from","rel","to"],
                "properties":{
                    "from":described("string", "Source memory, the containing record in the request."),
                    "rel":described("string", "Compiled relation type."),
                    "to":described("string", "Target memory addressed by connect_to.ref.")
                }
            }
        }),
        "relation_quality": described("array", "Preview per-relation validation. After commit, recover it with receipt.action."),
        "relation_quality_metrics": described("object", "Preview counts and prior-context coverage; stored in the receipt after commit."),
        "ingest_preview": described("object", "Canonical kmp_ingest arguments. Present only on dry-run."),
        "diagnostics": described("array", "Planner diagnostics that qualify the write."),
        "next_suggested_reads": described("array", "Optional preview reading suggestions. Accepted writes need no routine verification; receipt.action retrieves audit detail on demand."),
        "viewer": output_object(json!({
            "url": described("string", "Loopback, read-only viewer URL carrying this session's capability."),
            "tell_the_user": described("string", "One-time handoff text for the human; it is not another kernel instruction.")
        }))
    }))
}
