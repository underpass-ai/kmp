use serde_json::{Value, json};

use crate::contract::schema::definition::tool_definition_with_output;
use crate::contract::schema::paging::page_schema;
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
        "kmp_trace",
        "Trace declared links between memory refs. A to array, search or temporal selection enables a bounded shared search through same-about entries. One shortest discovered route per destination by default; search can retain alternatives and follow each relation type in its chosen direction; paths do not establish answer completeness. A single to without search or temporal selection retains equivalence-aware tracing.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["about", "from", "to"],
            "properties": {
                "about": string_schema("Memory anchor for this trace. Refs normally belong to it; a trace may cross a writer-declared same_event_as or same_entity_as link backed by a kmp_relate proposal. Naming another about's ref alone does not create a path."),
                "from": string_schema("Source memory ref. In live gRPC mode this must resolve to a kernel node id."),
                "to": json!({"description": "Destination ref, or 1..8 distinct same-about entry refs for bounded search.",
                    "oneOf": [{"type":"string","minLength":1}, {"type":"array","minItems":1,"maxItems":8,"uniqueItems":true,"items":{"type":"string","minLength":1}}]}),
                "search": json!({"type":"object","additionalProperties":false,
                    "description":"Bounded native traversal on the selected clock. Shared work limits; page/budget control transport separately. Temporal admission uses as_of/interval/axis; no cross-about expansion in this mode.",
                    "properties": {
                        "max_nodes":{"type":"integer","minimum":1,"maximum":4096,"default":256,"description":"Distinct discovered refs, including excluded endpoints and coordinate labels; reserve before reading."},
                        "max_edges":{"type":"integer","minimum":1,"maximum":32768,"default":2048,"description":"Adjacency rows decoded, including coordinate and filtered rows."},
                        "max_depth":{"type":"integer","minimum":1,"maximum":1024,"default":128},
                        "select": material_selection_schema(),
                        "dimensions": trace_dimensions_schema(false),
                        "prefer_dimensions": trace_dimensions_schema(true),
                        "paths_per_target":{"type":"integer","minimum":1,"maximum":8,"default":1,"description":"Up to this many simple candidate paths per target. Quotas and work limits can omit a better joint proof."},
                        "max_states":{"type":"integer","minimum":1,"maximum":32768,"default":4096,"description":"Root plus eligible path extensions attempted, including cycle and visited-node rejections."},
                        "follow":{"type":"array","minItems":1,"maxItems":16,"uniqueItems":true,"description":"Allowed moves instead of direction/relations. Unordered; does not require a sequence of types.","items":{"type":"object","additionalProperties":false,"required":["rel","direction"],"properties":{"rel":{"type":"string","minLength":1},"direction":{"type":"string","enum":["outgoing","incoming"]}}}},
                        "direction":{"type":"string","enum":["outgoing","incoming"],"default":"outgoing","description":"Traversal direction; stored source/target and meaning are preserved."},
                        "relations":{"type":"array","uniqueItems":true,"items":{"type":"string"},"description":"Exact relation types to follow. Omitted/empty admits every non-structural link with stored why and evidence."}
                    }}),
                "as_of": as_of_schema(),
                "interval": interval_schema(),
                "axis": recall_axis_schema(),
                "role": string_schema("Optional caller role."),
                "goal": string_schema("Optional trace goal."),
                "page": page_schema("Maximum number of trace relations to return in this page."),
                "budget": budget_schema(1_600, 1)
            }
        }),
        trace_output_schema(),
    )
}

fn trace_output_schema() -> Value {
    output_object(json!({
        "summary": described("string", "Concise statement of the path selection."),
        "trace": described("array", "Typed relation table. In bounded mode, routes index this complete table after all pages are joined; empty at a work cutoff does not prove no path."),
        "search": json!({"type":"object","description":"Present only for bounded mode. Stop reason distinguishes targets_reached, frontier_exhausted, node/edge/depth/state_budget and source_outside_selection. Optional routing reports dimensional checks/rejections, priority/FIFO pops and preferred_route_entries aligned with returned routes (including source). dimension_focus_v1 changes order, never drops a state by score; no shortest-path guarantee. A dense adjacency may exhaust N/E before queued preferred states expand. Optional material reports candidate count, selected original indexes, material nodes, covered/incomplete group indexes, benefit and selection work. Group indexes follow supplied groups; without groups they follow sorted target refs. expand_candidates is an optional full call starting a new selection without select, not a page continuation. Zero selected paths can mean no group fit, even with candidates available. Includes work counters, source from, direction (per_relation when follow is used), follow moves and paths_per_target. unreached_targets have no route; incomplete_targets have fewer than their quota. targets_reached means quotas obtained; the source needs only its zero-hop route. Frontier/leaf exhaustion is relative to selected direction, types and about; it proves no semantic answer. coordinate_rows are included in scanned_edges. A false temporal_selection_resolved means the ref cut could not be resolved within budget. clock_unknown_edges index undated selected links and do not prove those links existed at the cut."}),
        "routes": described("array", "Present in bounded mode. Each candidate has a target and zero-based edge_indexes into the complete trace table, in traversal hop order. Join every page first. Start at search.from and match each stored endpoint to reconstruct traversal direction. An empty index list is a zero-hop route to from. With search.select, only selected candidate paths and their complete relations are returned."),
        "page": relation_page_output_schema("trace relations", "Opaque trace cursor; repeat it as page.cursor with selection arguments unchanged. budget.max_bytes and page.entries may vary; changed selected content or arguments return a conflict with a complete restart action."),
        "quality": nullable_output_schema(quality_output_schema(), "Response-shape metrics; null when the backend supplied none."),
        "next_actions": described("array", "Complete tool/arguments calls that continue this exact selection or raise an insufficient byte allowance; empty at the end. Execute without reconstructing filters or cursors."),
        "warnings": warnings_output_schema()
    }))
}

fn material_selection_schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["max_material_nodes"],
    "description":"Select complete declared groups from bounded candidate paths before returning full links. Fixed beam B4/alpha0.5; heuristic, not a guarantee of optimality or semantic proof. Separate material budget from discovered nodes.",
    "properties":{
        "max_material_nodes":{"type":"integer","minimum":1,"maximum":4096,"description":"Unique entry refs on selected paths, including from. Does not count tokens or source bodies."},
        "max_paths":{"type":"integer","minimum":1,"maximum":8,"default":4},
        "groups":{"type":"array","maxItems":8,"description":"Caller-declared requirements. Omitted/empty: one equally weighted group per target. Each alternative is an AND set; alternatives are OR. Every ref must appear in to.",
            "items":{"type":"object","additionalProperties":false,"required":["alternatives"],"properties":{
                "weight":{"type":"integer","minimum":1,"maximum":1000,"default":1},
                "alternatives":{"type":"array","minItems":1,"maxItems":8,"items":{"type":"array","minItems":1,"maxItems":8,"uniqueItems":true,"items":{"type":"string","minLength":1}}}
            }}}
    }})
}

fn trace_dimensions_schema(preferred: bool) -> Value {
    let mut schema = dimensions_schema();
    schema["properties"]["scope"] = json!({"type":"string","enum":["current_about"]});
    schema["properties"]
        .as_object_mut()
        .expect("properties")
        .remove("abouts");
    for key in ["include", "exclude", "scope_ids"] {
        schema["properties"][key]["maxItems"] = json!(64);
    }
    schema["properties"]["selectors"]["maxItems"] = json!(16);
    schema["properties"]["selectors"]["items"]["properties"]["values"]["maxItems"] = json!(64);
    schema["properties"]["selectors"]["description"] = json!(
        "Conjoined predicates on the complete set of coordinates admitted on the selected clock. Absence refers to that set, not proof that no other membership exists."
    );
    schema["description"] = json!(if preferred {
        "Soft order, not exclusion: three pops maximize (matches, -depth, -discovery index), then one FIFO pop. Requires a kind, scope value or selector. May reach a longer route first; no shortest-path guarantee. Coordinates share N/E. Finite limits can still omit nonpreferred bridges."
    } else {
        "Hard filter on source and every traversed entry, using only coordinates admitted on the selected clock. It can remove a necessary bridge; use prefer_dimensions for a preference. Reuses native only/except/scopes/selectors. Coordinate work shares N/E."
    });
    schema
}
