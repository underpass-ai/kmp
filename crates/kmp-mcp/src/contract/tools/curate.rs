use serde_json::{Value, json};

use crate::contract::schema::definition::tool_definition_with_output;
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
        false,
        "Review the relations of one or several abouts with the calling agent as author and TypeSafe Jev as a second reader. `mode: review` reads the selection the way kmp_relate does and writes nothing. It returns `missing`: pairs of current facts nothing declares yet — paired by the kernel from checkable signals (rare identifiers, summaries, names, shared labels), inside one about or across abouts, or picked by Jev for facts nothing else paired — each with the relation type Jev would choose, or `contradicts` when the two cannot both be true; and `suspect`: declared relations whose why and evidence Jev does not find supporting, or that Jev would type differently. Jev only chooses among offered options and judges text; it never writes a relation or a word of one, so every why and evidence you declare is yours, and a suggested type is a suggestion. Across abouts only same_event_as and same_entity_as are ever suggested. Jev is opt-in per store: `typesafe.json` beside the store and `TYPESAFE_API_KEY` in the environment; fact text of the selection is sent to TypeSafe. Without it the review returns kernel pairs untyped and audits nothing. The review is frozen under `review_token`; page it with that token and `page.cursor`. `mode: apply` takes items of a frozen review with your why and evidence; Jev reads your text once more and withholds what it doubts until you confirm it; the rest goes through kmp_write_memory's own plan, neighbourhood review and idempotency, and a needs_review answer resumes here. Only items whose `from` belongs to `about` are written by a call. Embedded store only.",
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
            "mode": {"type": "string", "enum": ["review", "apply"], "description": "`review` reads, judges and freezes; it writes nothing. `apply` writes the accepted items of a frozen review through the writer's own review."},
            "about": string_schema("The current about: the review's root and the first of the selected abouts."),
            "dimensions": dimensions_schema(),
            "interval": interval_schema(),
            "axis": recall_axis_schema(),
            "budget": budget_schema(2_400, 2),
            "max_pairs": {"type": "integer", "minimum": 1, "maximum": 40, "description": "Most missing relations to return, strongest Jev confidence first. Default 12."},
            "review_token": {"type": "string", "pattern": "^[0-9a-f]{64}$", "description": "A token a review returned. With it, the call pages that frozen review and reads or judges nothing again."},
            "page": page_schema("Maximum number of items from missing then suspect to return in this page; default 8, at most 20."),
            "accepted": {"type": "array", "minItems": 1, "description": "apply: the missing items you declare, each in your own words. Suspect items cannot be applied.", "items": {"type": "object", "additionalProperties": false, "required": ["item_id", "why", "evidence"], "properties": {
                "item_id": string_schema("An id `m<n>` from this review."),
                "why": string_schema("Your one checkable sentence for the link. Jev never writes it."),
                "evidence": string_schema("What in the sources shows the link."),
                "confidence": {"type": "string", "enum": ["high", "medium", "low", "unknown"]},
                "rel": string_schema("Overrides the suggested type; a relation a writer may declare, and across abouts only same_event_as or same_entity_as."),
                "reverse": {"type": "boolean", "description": "Declare it from `to` to `from`."},
                "confirm_doubted": {"type": "boolean", "description": "Write it although Jev doubted it in an earlier apply of these items."}
            }}},
            "actor": string_schema("apply: writer name; defaults to the persistent agent name when context_id is supplied."),
            "write_review_token": {"type": "string", "description": "apply: the neighbourhood review token a needs_review answer returned; its next action carries it."},
            "idempotency_key": string_schema("apply: logical write identity; a needs_review answer's next action carries the one to reuse.")
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
        "suspect": described("array", "review: declared relations Jev doubts: `item_id`, `from`, `to`, the declared `rel`, `support` (Jev's probability that its why and evidence hold), `suggested_rel` and `jev`. Reported only; relations are never retracted here."),
        "jev": described("object", "review: model, requests and input tokens spent; null when Jev was not used."),
        "page": described("object", "review: `entries`, `total` items in the review and `next_cursor`, null on the last page."),
        "curate": described("object", "apply: `doubted` items Jev asks you to reread (`item_id`, `support`, `suggested_rel`, `jev`) — withheld until you correct them or send `confirm_doubted` —, `rejected` items this call cannot write with the reason, the `jev` usage of the pre-write check, and `warnings`."),
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
