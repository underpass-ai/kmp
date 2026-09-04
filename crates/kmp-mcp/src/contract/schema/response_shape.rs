//! Response shapes more than one tool advertises.
//!
//! One concept: the blocks that recur across verbs — a page, a proof, a
//! projection envelope, a truncation report, a quality report, warnings. A
//! shape used by a single verb belongs with that verb, not here; these earn
//! their place by having two or more.
//!
//! Depends only on `schema`, never on a tool.

use serde_json::{Value, json};

use super::primitives::{described, nullable_described, output_object, string_array};

pub(crate) fn warnings_output_schema() -> Value {
    string_array(
        "Operational warnings. A non-empty list qualifies the success and must not be discarded.",
    )
}
pub(crate) fn recall_envelope_properties() -> Value {
    json!({
        "projection": projection_output_schema(),
        "truncation": truncation_output_schema(),
        "warnings": warnings_output_schema()
    })
}
/// `page`, with what `total` counts said out loud.
///
/// It counts different things in different verbs — expansion items in a
/// recall, temporal entries in a move — and nothing in the surface said the
/// unit changed. A number whose meaning the receiver has to guess is worse
/// than no number, because it will be acted on.
pub(crate) fn page_output_schema(unit: &str, cursor_description: &str) -> Value {
    output_object(json!({
        "returned": described("integer", "How many items this response carries."),
        "total": described("integer", &format!("How many {unit} the selection holds in total.")),
        "has_more": described(
            "boolean",
            "Whether the slice was cut. A partial answer reported as a whole one is the failure \
             this field exists to prevent."
        ),
        "next_cursor": nullable_described("string", cursor_description)
    }))
}
/// `proof`, which is where a caller decides whether to believe the answer.
pub(crate) fn proof_output_schema(confidence_description: &str) -> Value {
    output_object(json!({
        "confidence": described("string", confidence_description),
        "evidence": described(
            "array",
            "Stored entry text or evidence, verbatim. `text` is the canonical body and \
             `metadata.proof_role` distinguishes the claim from its evidence. `metadata` may \
             also say how an item was retrieved, never what it says. `reached_by` (relation, \
             association or bridge) with `reached_from`, `reached_via` and `reached_hops` marks \
             an item the question never matched on its own words: proof, not answer, and never \
             cited in `because`. On a cited item, `bridged_terms` names the word pairs the \
             lexical-bridge table crossed a language with (`valvula≈valve 0.51`); \
             `restated_from` and `restated_via` name a memory a writer declared this one \
             restates; `matched_via: summary` with `summary_terms` says the question reached it \
             through the writer's English `summary_en` and not through its text, and names the \
             question's words the summary supplied. Read those as the writer's words, not the \
             memory's."
        ),
        "missing": described(
            "array",
            "What was looked for and not found. Non-empty alongside UNKNOWN, and it says which \
             kind: nothing retrieved at all, or nothing that bears on the question."
        ),
        "superseded": described(
            "array",
            "Entries a later one replaced, each with `superseded_by` and the `why`. A lifecycle, \
             not a disagreement: read the older entry as what was true then, not as advice."
        ),
        "expired": described(
            "array",
            "Historical entries whose exclusive `valid_until` had passed where the read \
             stood: the cursor on a temporal move, `as_of` or the interval's end on wake and \
             ask, else the memory's own latest instant. Expiry needs no replacement, so this \
             is separate from `superseded`."
        ),
        "conflicts": described(
            "array",
            "Entries that explicitly contradict each other and are both still live. The tension \
             is the information — this is deliberately not the same field as `superseded`."
        ),
        "matched_relations": described(
            "array",
            "Which typed relations contributed to the ordering. Relation prose can improve a \
             match and can never promote unrelated evidence into an answer."
        ),
        "matched_terms": described("array", "Question terms that matched retrieved evidence."),
        "path": described("array", "The traversal that connects the cited evidence."),
        "frontier_size": described(
            "integer",
            "How much was reachable and not returned, which is the signal to expand."
        ),
        "interval": nullable_described(
            "object",
            "The half-open span the recall stood within, `start` and `end`, when the caller \
             asked for one; null otherwise."
        ),
        "axis": nullable_described(
            "string",
            "The clock `as_of` or `interval` read on — `occurred`, `observed`, `ingested`, \
             `validity`, or `default` for the compatible precedence; null when the recall \
             stood at the memory's own frontier."
        ),
        "as_of": nullable_described(
            "string",
            "The instant the recall stood at, when the caller asked for one; a `ref` cursor is \
             reported as the instant it resolved to. Null otherwise."
        ),
        "nearest_outside": nullable_described(
            "object",
            "On UNKNOWN within an interval: the closest match outside it — its `ref`, its \
             `time` and the `axis` that instant was read on — so a reader can tell \"not then\" \
             from \"not known\". Null otherwise."
        )
    }))
}
/// `projection`, the budget envelope on a recall.
fn projection_output_schema() -> Value {
    let mut page = page_output_schema(
        "eligible expansion items",
        "Opaque selection-bound recall cursor, or null. Repeat every other bound argument unchanged as page.cursor; only page.entries and budget token/byte ceilings may vary.",
    );
    page["properties"]["offset"] = described(
        "integer",
        "Number of eligible expansion items reconstructed by earlier pages.",
    );
    output_object(json!({
        "contract": described("string", "The projection contract version, e.g. kmp.recall.projection.v1."),
        "budget": described("object", "The normative byte ceiling, bytes actually used, and retained token-planning hint."),
        "detail": described("string", "compact | balanced | full — the detail tier that was served."),
        "excluded_by_detail": described(
            "integer",
            "Items a richer `budget.detail` would have included. Not a truncation: they were \
             never eligible at this tier."
        ),
        "next_action": nullable_described(
            "string",
            "The exact call that continues this page, or null when there is nothing after it."
        ),
        "page": page,
        "sections": described("object", "Per-section core, returned, eligible, and total counts for reconstructing the full proof."),
        "selection_omitted": described("integer", "Items excluded by budget.max_entries before paging."),
        "core_text_shortened": described("boolean", "Whether stable core prose had to be shortened to fit max_bytes.")
    }))
}
fn truncation_output_schema() -> Value {
    output_object(json!({
        "truncated": described("boolean", "Always true when this optional object is present."),
        "token_limit": described("integer", "Advisory token-planning hint retained for compatibility; it does not filter the canonical structuredContent."),
        "byte_limit": described("integer", "Normative serialized-byte ceiling applied."),
        "omitted": described("object", "Exact counts by cause: page, prior page, remaining page, detail tier, selection cap, and shortened core text.")
    }))
}
pub(crate) fn quality_output_schema() -> Value {
    output_object(json!({
        "nodes": described("integer", "Returned node count."),
        "relationships": described("integer", "Returned relation count."),
        "details": described("integer", "Returned node-detail count."),
        "causal_density": described(
            "number",
            "Share of returned relations that explain rather than merely connect. Low means the \
             memory is a list; it is a property of what was written, not of this call."
        ),
        "detail_coverage": described("number", "Share of returned nodes that carry stored detail."),
        "truncated": described("boolean", "Whether the rendering dropped anything.")
    }))
}
