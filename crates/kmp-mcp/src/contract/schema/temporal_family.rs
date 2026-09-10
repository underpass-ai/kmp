use serde_json::{Value, json};

use crate::contract::schema::definition::tool_definition_with_output;
#[allow(unused_imports)]
use crate::contract::schema::primitives::*;
use crate::contract::schema::primitives::{
    nullable_described, nullable_output_schema, output_object, string_array,
};
#[allow(unused_imports)]
use crate::contract::schema::relation_vocabulary::*;
#[allow(unused_imports)]
use crate::contract::schema::request_shape::*;
#[allow(unused_imports)]
use crate::contract::schema::response_shape::*;
use crate::contract::schema::response_shape::{
    page_output_schema, proof_output_schema, quality_output_schema, warnings_output_schema,
};
use crate::contract::temporal_entry_field::TemporalEntryField;
pub(crate) fn temporal_tool_definition(name: &str, description: &str, cursor_key: &str) -> Value {
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
            "refs": {"type":"array", "minItems":1, "uniqueItems":true, "items":{"type":"string","minLength":1},
                "description":"Optional memory refs to focus entry selection before entry/window limits. Keep the question clock and cutoff; refs neither override scope/labels/time nor filter dependency sources. Omit for all matching entries."},
            "fields": {"type":"array", "uniqueItems":true,
                "items":{"type":"string","enum":TemporalEntryField::ALL},
                "description":"Choose entry fields; omit for all. ref and kind always remain. Omitted fields are declared in selection.fields and each reduced entry supplies detail_action. Only entries are projected: proof and raw audit data are controlled separately by include."},
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
                    "dependencies": {"type":"boolean", "description":"Include writer-declared memory dependencies and their actual stored bodies/proof, up to two hops and eight members per seed. Implies evidence and relations; do not set them false. Same abouts, filters and clock cutoff. proof.groups reports anonymous unavailable/limit counts, not semantic sufficiency. Complete all response pages before consuming a group."},
                    "raw_refs": {
                        "type": "boolean",
                        "description": "Return typed raw audit refs for selected temporal entries, unaffected by fields."
                    }
                }
            },
            "depth": integer_schema("Optional graph traversal depth. Applies in embedded and live gRPC modes; it overrides budget.depth."),
            "page": {
                "type":"object", "additionalProperties":false,
                "properties": {
                    "cursor":string_schema("Opaque response-page cursor; execute next_actions to keep the selection bound. Changed arguments or content require a fresh read."),
                    "entries":{"type":"integer","minimum":1,"description":"Maximum complete expansion items on this response page, including entries and proof."}
                }
            },
            "budget": budget_schema(2_400, 3)
        }
    });
    input_schema["properties"][cursor_key] = cursor_schema;
    let mut interval = interval_schema();
    interval["minProperties"] = json!(1);
    interval["description"] = json!(
        "Half-open [start,end) entry selection on axis, with at least one bound. Validity selects overlapping spans. Forward/Rewind can start with interval alone; returned continuations preserve it."
    );
    input_schema["properties"]["interval"] = interval;
    if matches!(name, "kmp_forward" | "kmp_rewind") {
        input_schema["required"] = json!(["about"]);
        input_schema["anyOf"] = json!([{"required":[cursor_key]}, {"required":["interval"]}]);
    }
    tool_definition_with_output(
        name,
        description,
        input_schema,
        temporal_output_schema(name, cursor_key),
    )
}

pub(crate) fn temporal_output_schema(_tool_name: &str, _cursor_key: &str) -> Value {
    let mut proof = proof_output_schema(
        "Temporal reads use medium when entries were returned and unknown when none were returned; this is not relation-writer certainty.",
    );
    proof["properties"]["entries"] = described(
        "array",
        "With include.dependencies, full stored dependency records and coordinates. These support the selected entries; they are not additional matches in the history window. fields does not shorten proof entries.",
    );
    proof["properties"]["groups"] = json!({"type":"array", "description":"With include.dependencies, bounded writer-declared neighborhoods. Members resolve in entries or proof.entries after all pages. Source passages and typed links remain in proof.evidence/path. Anonymous counts describe only the selected scoped graph, never semantic sufficiency.", "items":output_object(json!({
        "seed_ref":described("string", "The selected temporal entry."),
        "member_refs":string_array("Seed and available dependency entries in deterministic discovery order."),
        "unavailable_in_selection":described("integer", "Distinct endpoints without an eligible body under this read; identities are not disclosed."),
        "omitted_by_limit":described("integer", "Distinct eligible frontier members not expanded because of hop or member limits."),
        "max_hops":described("integer", "Maximum expansion hops from this seed."),
        "max_members":described("integer", "Maximum members, including the seed.")
    }))});
    let mut page = page_output_schema(
        "entry and proof expansion items",
        "Opaque cursor for page.cursor; execute the complete next_actions call.",
    );
    let properties = page["properties"].as_object_mut().expect("page properties");
    properties.insert(
        "offset".into(),
        described("integer", "Expansion items reconstructed before this page."),
    );
    properties.insert("sections".into(), described("object", "Per-section returned_on_page, remaining and total counts. Append each section across pages before concluding from proof."));
    properties.insert("minimum_progress_bytes".into(), described("integer", "Present when the next complete item cannot fit. next_actions supplies a retry with this budget that admits at least one item."));
    output_object(json!({
        "summary": described("string", "Concise description of the temporal selection."),
        "next_actions": {"type":"array", "description":"Execute these calls unchanged. While page.has_more, reconstruct this packet first; once complete, these calls navigate remaining history (Near can offer both directions).", "items":output_object(json!({
            "tool":described("string", "Native temporal tool to execute."),
            "arguments":described("object", "Complete bound arguments, including the continuation cursor or a required budget adjustment.")
        }))},
        "selection": output_object(json!({
            "fields":output_object(json!({"included":string_array("Fields returned on each entry; ref and kind always remain."), "omitted":string_array("Entry fields available through each returned detail_action; omitted fields are not empty values.")})),
            "scope":described("string", "selected_packet: top-level summary, coverage and quality refer to this bounded selection, not the current response page or all memory."),
            "entries":described("integer", "Entries selected for this packet before response pagination."),
            "requested_refs":string_array("Present with refs: the explicit entry focus. Dependency records may have other refs within the same scope and time."),
            "unmatched_ref_count":described("integer", "Requested refs not matching this history selection; may be absent, out of scope/filter/clock/direction. Does not prove nonexistence. Independent of response-page completion."),
            "matching_entries":described("integer", "Matching temporal entries reported by the kernel before its entry/window limit."),
            "has_more":described("boolean", "More matching history remains outside this packet. Complete its response pages before following the returned navigation actions.")
        })),
        "temporal": nullable_described("object", "Direction, clock axis and entry interval. requested/resolved are null for a direct interval without an initial cursor."),
        "coverage": output_object(json!({
            "requested": nullable_described("object", "Dimension selection requested by the caller."),
            "included": string_array("Dimension scope ids included in the result."),
            "missing": string_array("Requested dimension scope ids not present in the result."),
            "dimensions": described("array", "Per-dimension returned counts and presence flags.")
        })),
        "entries": described("array", "Temporal entries in traversal order. ref/kind always remain; fields selects text, coordinates and metadata. Each reduced entry has an executable detail_action for a fresh full entry read in the same scope, clock and interval."),
        "page": page,
        "raw_refs": described("array", "Typed raw audit refs for selected entries when include.raw_refs=true."),
        "proof": proof,
        "quality": nullable_output_schema(quality_output_schema(), "Response-shape metrics; null when the backend supplied none."),
        "warnings": warnings_output_schema()
    }))
}
