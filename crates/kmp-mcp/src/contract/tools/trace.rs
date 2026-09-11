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
        "Choose one mode: to connects known destination refs; search.seek discovers compatible evidence paths from a seed without destinations. Never combine their search options. A to array, search or temporal selection enables a bounded shared search through same-about entries. One shortest discovered route per destination by default; search can retain alternatives and follow each relation type in its chosen direction; paths do not establish answer completeness. A single to without search or temporal selection retains equivalence-aware tracing.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["about", "from"],
            "oneOf": [{"required":["to"],"not":{"properties":{"search":{"required":["seek"]}},"required":["search"]}}, {"required":["search"],"properties":{"search":{"required":["seek"]}},"not":{"required":["to"]}}],
            "properties": {
                "about": string_schema("Memory anchor for this trace. Refs normally belong to it; a trace may cross a writer-declared same_event_as or same_entity_as link backed by a kmp_relate proposal. Naming another about's ref alone does not create a path."),
                "from": string_schema("Source memory ref. In live gRPC mode this must resolve to a kernel node id."),
                "to": json!({"description": "Destination ref, or 1..8 distinct same-about entry refs for bounded search.",
                    "oneOf": [{"type":"string","minLength":1}, {"type":"array","minItems":1,"maxItems":8,"uniqueItems":true,"items":{"type":"string","minLength":1}}]}),
                "search": trace_search_schema(),
                "as_of": as_of_schema(),
                "interval": interval_schema(),
                "axis": recall_axis_schema(),
                "role": string_schema("Optional caller role."),
                "goal": string_schema("Optional trace goal."),
                "page": page_schema("Maximum whole items in trace, then candidates, then groups for seek; relations alone otherwise."),
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
        "search": json!({"type":"object","description":"Present only for bounded mode. Stop reason distinguishes targets_reached, frontier_exhausted, node/edge/depth/state_budget and source_outside_selection. Optional routing reports dimensional checks/rejections, priority/FIFO pops and preferred_route_entries aligned with returned routes (including source). dimension_focus_pages_v2 alternates pages of at most four raw relations with child exploration, retaining exact positions and cached alternative rows. Routing also reports adjacency_pages, coordinate_pages and resumed_states (expansion turns of an already started path). It never drops a state by score; no shortest-path guarantee. Reading a page or its label coordinates can still exhaust N/E before a useful state expands. Optional material reports candidate count, selected original indexes, material nodes, covered/incomplete group indexes, benefit and selection work. Group indexes follow supplied groups; without groups they follow sorted target refs. expand_candidates is an optional full call starting a new selection without select, not a page continuation. Zero selected paths can mean no group fit, even with candidates available. Includes work counters, source from, direction (per_relation when follow is used), follow moves and paths_per_target. unreached_targets have no route; incomplete_targets have fewer than their quota. targets_reached means quotas obtained; the source needs only its zero-hop route. Frontier/leaf exhaustion is relative to selected direction, types and about; it proves no semantic answer. coordinate_rows are included in scanned_edges. A false temporal_selection_resolved means the ref cut could not be resolved within budget. clock_unknown_edges index undated selected links and do not prove those links existed at the cut."}),
        "seek": described("object", "Seed evidence selection: status compatible, ambiguous, review_required, missing_obligation, incompatible_obligations or partial. declared_obligations_complete requires a known compatible group and is always false for contextual discovery; it never means answer completeness or unique identity. Reports missing_roles, work/cut counters, role order and complete candidate/group counts. Optional review_context is a new scoped Goto read, separate from mandatory page continuations. No source-body materialization or semantic inference."),
        "candidates": described("array", "Seek paths with stable index, role, anchor, witness, context_hops, nodes and directed trace edge_indexes. Anchor is the main relation traversal start; witness is its other endpoint. Bindings give label/reference domains, roles and explicit endpoints when anchor equality is used; missing names exact witness refs and keys. Paths outside groups cannot jointly satisfy the request. clock_unknown keeps historical presence unproven. Join all pages."),
        "groups": described("array", "Compatible joint alternatives with stable index, candidate_indexes and intersected bindings. Missing values remain unknown even when another role has a known domain; clock_unknown requires review. All indexes address the complete tables, not a page. Multiple proofs of the same bindings need not mean ambiguity."),
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
        "Soft order, not exclusion: three priority turns then one FIFO turn. Read at most four raw relations per turn; unfinished parents resume at child depth after newly queued children. Priority maximizes (matches, -effective depth, -enqueue order). Requires a kind, scope value or selector. May reach a longer route first; no shortest-path guarantee. Whole coordinate reads share N/E. Finite limits can still omit bridges or late unread edges."
    } else {
        "Hard filter on source and every traversed entry, using only coordinates admitted on the selected clock. It can remove a necessary bridge; use prefer_dimensions for a preference. Reuses native only/except/scopes/selectors. Coordinate work shares N/E."
    });
    schema
}

fn trace_search_schema() -> Value {
    let mut schema = json!({"type":"object","additionalProperties":false,
    "description":"Choose exactly one mode below. Known destinations use to and destination options; evidence discovery uses seek and witness joins, without to. Only max_nodes/max_edges/max_depth/max_states are shared search fields. as_of/interval/axis select time at call level; page/budget page the result, not the search. Same-about traversal.",
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
    }});
    schema["properties"]["seek"] = super::trace_seek::roles_schema();
    schema["properties"]["same_labels"] = json!({"type":"array","maxItems":16,"uniqueItems":true,"items":{"type":"string","minLength":1},"description":"With seek, require common values for these label keys across all role witnesses. Missing stays unknown. A label never proves identity."});
    schema["properties"]["same_ref"] = super::trace_seek::same_ref_schema();
    let evidence_fields = ["seek", "same_labels", "same_ref"];
    let work_fields = ["max_nodes", "max_edges", "max_depth", "max_states"];
    let properties = schema["properties"]
        .as_object_mut()
        .expect("search properties");
    let mut destinations = Vec::new();
    let mut evidence = Vec::new();
    for (key, field) in properties {
        if work_fields.contains(&key.as_str()) {
            destinations.push(key.clone());
            evidence.push(key.clone());
        } else {
            let mode = if evidence_fields.contains(&key.as_str()) {
                evidence.push(key.clone());
                "Evidence mode only. "
            } else {
                destinations.push(key.clone());
                "Destination mode only; not with seek. "
            };
            let description = field["description"].as_str().unwrap_or_default();
            field["description"] = json!(format!("{mode}{description}"));
        }
    }
    // Declare field bodies once: clients see two exclusive choices while the
    // existing unknown-argument validator retains recursive property checks.
    schema["oneOf"] = json!([
        {"title":"Known destinations", "description":"Requires to at call level. No seek or witness joins.",
         "propertyNames":{"enum":destinations}},
        {"title":"Evidence from seed", "description":"Requires seek; no to. Relations and constraints live inside roles.",
         "required":["seek"], "propertyNames":{"enum":evidence}}
    ]);
    schema
}
