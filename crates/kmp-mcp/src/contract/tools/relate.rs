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
        "kmp_relate",
        "Read what the memories of several abouts have to do with each other, off what they share and without declaring anything. Abouts are never joined by relations, so this reads the coordinates: the dimension scopes two abouts both use, and where on one clock each memory stands inside them. Returns the facts of the selected abouts within the interval with the lifecycle state they had at its end, the relations each about declared between its own facts, coordinate relations between facts of different abouts inside a scope they share (before, after, during, concurrent, same sequence, same rank, or merely the shared scope), and tensions: facts that still stand and that a declared `contradicts` joins. Nothing is ranked by a question and nothing is generated; every relation says why in one sentence a reader can check against the coordinates. Pages by position over facts, declared, coordinate and tensions in that order.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["about"],
            "properties": {
                "about": string_schema("The current about: the read's root and the first of proof.abouts_selected."),
                "dimensions": dimensions_schema(),
                "interval": interval_schema(),
                "axis": recall_axis_schema(),
                "budget": budget_schema(2_400, 2),
                "page": page_schema()
            }
        }),
        relate_output_schema(),
    )
}

fn relate_output_schema() -> Value {
    output_object(json!({
        "summary": described("string", "How many facts of how many abouts were related on which clock, and how many relations of each kind; or that nothing fell within the selection, naming the nearest fact outside it."),
        "facts": described("array", "The entries of the selected abouts within the selection: `ref`, `about`, `kind`, `text` verbatim, `coordinates`, and `state` — `current`, `superseded` (with `superseded_by`) or `expired` (with `valid_until`) — as the lifecycle stood at the end of the interval."),
        "declared": described("array", "The relations each about declared between its own facts, cut to the selection, with class, why, evidence and confidence. Abouts declare nothing across each other, so none of these crosses an about."),
        "coordinate": described("array", "Relations between facts of different abouts read off their coordinates inside a scope they share: `from`, `to`, `kind` (`shares_scope`, `before`, `after`, `during`, `concurrent`, `same_sequence`, `same_rank`), the bare `scope_id`, the `axis` read, and `why` in one checkable sentence. Declared by nobody."),
        "tensions": described("array", "Facts that both still stand and that a declared `contradicts` joins: `ref`, `other`, the `scope_id` they share when they share one, and the `why` and `evidence` of the declaration. Shown, never resolved."),
        "proof": proof_output_schema("Unspecified: a relate reading ranks nothing by a question. `interval`, `axis`, `abouts_selected`, `abouts_empty_in_selection`, `superseded`, `expired` and, when nothing fell within the selection, `nearest_outside` are what it declares."),
        "page": page_output_schema("facts, declared, coordinate and tensions, by position in that order", "Opaque relate cursor; repeat it as page.cursor with every other argument unchanged."),
        "warnings": warnings_output_schema()
    }))
}
