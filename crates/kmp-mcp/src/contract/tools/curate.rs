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
        "Review the relations of one or several abouts with the calling agent as author and TypeSafe Jev as a second reader. `mode: review` reads the selection the way kmp_relate does and writes nothing. It returns `missing`: pairs of current facts nothing declares yet — paired by the kernel from checkable signals (rare identifiers, summaries, names, shared labels), inside one about or across abouts, or picked by Jev for facts nothing else paired — each with the relation type Jev would choose, or `contradicts` when the two cannot both be true; and `suspect`: declared relations whose why and evidence Jev does not find supporting, or that Jev would type differently. Jev only chooses among offered options and judges text; it never writes a relation or a word of one, so every why and evidence you declare is yours, and a suggested type is a suggestion. Across abouts only same_event_as and same_entity_as are ever suggested. Jev is opt-in per store: `typesafe.json` beside the store and `TYPESAFE_API_KEY` in the environment; fact text of the selection is sent to TypeSafe. Without it the review returns kernel pairs untyped and audits nothing. The review is frozen under `review_token`; page it with that token and `page.cursor`. Embedded store only.",
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
            "mode": {"type": "string", "enum": ["review"], "description": "`review` reads, judges and freezes; it writes nothing."},
            "about": string_schema("The current about: the review's root and the first of the selected abouts."),
            "dimensions": dimensions_schema(),
            "interval": interval_schema(),
            "axis": recall_axis_schema(),
            "budget": budget_schema(2_400, 2),
            "max_pairs": {"type": "integer", "minimum": 1, "maximum": 40, "description": "Most missing relations to return, strongest Jev confidence first. Default 12."},
            "review_token": {"type": "string", "pattern": "^[0-9a-f]{64}$", "description": "A token a review returned. With it, the call pages that frozen review and reads or judges nothing again."},
            "page": page_schema("Maximum number of items from missing then suspect to return in this page; default 8, at most 20.")
        }
    })
}

fn curate_output_schema() -> Value {
    output_object(json!({
        "summary": described("string", "How many missing and suspect relations over how many current facts, and whether Jev was used."),
        "review_token": described("string", "The frozen review's token: pass it back to page this review."),
        "missing": described("array", "Pairs nothing declares: `item_id`, `from` and `to` as {ref, about, excerpt}, `suggested_rel` (Jev's choice, `contradicts`, or null without Jev), `proposed_by` (`kernel` or `jev`), the kernel `signals` and its `pairing_why` (null for Jev pairs), and `jev` {confidence, top three options}."),
        "suspect": described("array", "Declared relations Jev doubts: `item_id`, `from`, `to`, the declared `rel`, `support` (Jev's probability that its why and evidence hold), `suggested_rel` and `jev`. Reported only; relations are never retracted here."),
        "jev": described("object", "Model, requests and input tokens spent on this review; null when Jev was not used."),
        "page": described("object", "`entries`, `total` items in the review and `next_cursor`, null on the last page."),
        "next_actions": described("array", "The exact call that returns the next page of this frozen review; empty on the last page."),
        "warnings": warnings_output_schema()
    }))
}
