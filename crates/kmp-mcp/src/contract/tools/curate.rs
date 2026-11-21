use serde_json::{Value, json};

use crate::contract::schema::definition::{ToolEffect, tool_definition_with_output};
use crate::contract::schema::paging::page_schema;
#[allow(unused_imports)]
use crate::contract::schema::primitives::*;
#[allow(unused_imports)]
use crate::contract::schema::request_shape::*;
#[allow(unused_imports)]
use crate::contract::schema::response_shape::*;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    tool_definition_with_output(
        "kmp_curate",
        ToolEffect::Write {
            destructive: false,
            idempotent: false,
        },
        "Find missing and doubtful relations with TypeSafe Jev as second reader (`review`, `paths`, `labels`), then declare those you confirm (`apply`).",
        curate_schema(),
        curate_output_schema(),
    )
}

pub(crate) fn curate_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["mode", "about"],
        "properties": {
            "mode": {"type": "string", "enum": ["review", "apply", "paths", "labels"], "description": "`review` reads, judges and freezes; it writes nothing. `paths` finds whole chains from `from` (to `to`, or as far as they go) over declared relations and steps Jev proposes; it writes nothing. `labels` proposes, for the facts in `focus`, label values their about already uses under keys they lack; it writes nothing, and `kmp_relabel` applies the ones you confirm. `apply` writes the accepted items of a frozen review — a proposed path step included — through the writer's own review."},
            "about": string_schema("The current about: the review's root and the first of the selected abouts."),
            "dimensions": dimensions_schema(),
            "interval": interval_schema(),
            "axis": recall_axis_schema(),
            "max_pairs": {"type": "integer", "minimum": 1, "maximum": 40, "description": "Most missing relations to return, strongest Jev confidence first. Default 12."},
            "review_token": {"type": "string", "pattern": "^[0-9a-f]{64}$", "description": "A token a review returned. With it, the call pages that frozen review and reads or judges nothing again."},
            "page": page_schema("Maximum number of items from missing then suspect to return in this page; default 8, at most 20."),
            "focus": {"type": "array", "minItems": 1, "maxItems": 8, "items": {"type": "string"}, "description": "labels: the facts to label (required). review: only the relations these facts are missing, typically the ones just written. Jev reads their whole about for direct partners."},
            "from": string_schema("paths: the fact a chain starts from, an entry ref of the selection."),
            "to": string_schema("paths: the fact the chain must reach. Omitted: the longest chains that follow from `from`."),
            "max_hops": {"type": "integer", "minimum": 1, "maximum": 12, "description": "paths: longest chain considered. Default 6."},
            "accepted": {"type": "array", "minItems": 1, "description": "apply: the missing items you declare, each in your own words. Suspect items cannot be applied.", "items": {"type": "object", "additionalProperties": false, "required": ["item_id", "why", "evidence"], "properties": {
                "item_id": string_schema("An id `m<n>` from this review."),
                "why": string_schema("Your one checkable sentence for the link. Jev never writes it."),
                "evidence": string_schema("What in the sources shows the link."),
                "confidence": {"type": "string", "enum": ["high", "medium", "low", "unknown"], "default": "unknown"},
                "rel": string_schema("Overrides the suggested type; a relation a writer may declare, and across abouts only same_event_as or same_entity_as."),
                "reverse": {"type": "boolean", "description": "Declare it from `to` to `from`."},
                "confirm_doubted": {"type": "boolean", "description": "Write it although Jev doubted it in an earlier apply of these items."}
            }}},
            "actor": string_schema("apply: writer name. Omitted: the MCP client name from initialize."),
            "write_review_token": {"type": "string", "description": "apply: the neighbourhood review token a needs_review answer returned; its next action carries it."},
            "idempotency_key": string_schema("apply: logical write identity; a needs_review answer's next action carries the one to reuse. Keys are global to the store, not per about.")
        }
    })
}

fn curate_output_schema() -> Value {
    // apply answers with the writer's own result, so its fields are the
    // writer's; review's fields and `curate` are added beside them.
    let mut schema = super::write_memory::write_memory_output_schema();
    let properties = schema["properties"].as_object_mut().expect("properties");
    for (key, value) in json!({
        "summary": described("string", "review: how many missing and suspect relations over how many current facts, and whether Jev was used. apply: what was written or why nothing was."),
        "review_token": described("string", "review: the frozen review's token; pass it back to page or apply this review."),
        "missing": described("array", "review: pairs nothing declares: `item_id`, `from` and `to` as {ref, about, excerpt}, `suggested_rel` (Jev's choice among the types a writer may declare, or null without Jev), `proposed_by` (`kernel` or `jev`), the kernel `signals` and its `pairing_why` (null for Jev pairs), and `jev` {confidence, top three options}."),
        "suspect": described("array", "review: declared relations Jev doubts: `item_id`, `from`, `to`, the declared `rel`, `support` (Jev's probability that its why and evidence hold), `reasons` (`support`, `type`), `suggested_rel` and `jev`. Reported only; relations are never retracted here."),
        "jev": nullable_described("object", "review: model, requests and input tokens spent; null when Jev was not used."),
        "page": described("object", "review: `entries`, `total` items in the review and `next_cursor`, null on the last page."),
        "paths": described("array", "paths: up to three chains, fewest proposed steps first: `hops` of {`from`, `to` as {ref, about, excerpt} in walking order, `rel`, `declared`, `reversed` when walked against its stored direction, `confidence`, and `item_id` for a proposed step, applicable with `mode: apply`}, plus `proposed` and `confidence`."),
        "proposed_labels": described("array", "labels: proposed memberships, strongest first: {`ref`, `key`, `value` among the values the about already uses, `confidence`, `excerpt`}. Apply the ones you confirm with kmp_relabel."),
        "avoided": described("array", "paths: declared relations the walk left out because Jev found their why and evidence do not hold (`from`, `to`, `rel`, `support`). Audit them with `mode: review`."),
        "curate": described("object", "apply: `doubted` items Jev asks you to reread (`item_id`, `support`, `direction` — how strongly Jev reads it the way you wrote it —, `reasons` among `support`, `type` and `direction`, `suggested_rel`, `jev`) — withheld until you correct them or send `confirm_doubted` —, `rejected` items this call cannot write with the reason, the `jev` usage of the pre-write check, and `warnings`."),
        "next_actions": described("array", "review: the call for the next page. apply after needs_review: the kmp_curate call that resumes this exact apply. Execute unchanged."),
    })
    .as_object()
    .expect("object")
    {
        properties.insert(key.clone(), value.clone());
    }
    if let Some(status) = properties.get_mut("status")
        && let Some(values) = status.get_mut("enum").and_then(Value::as_array_mut)
    {
        values.push(json!("nothing_written"));
    }
    schema
}
