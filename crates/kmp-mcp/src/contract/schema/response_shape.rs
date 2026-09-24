//! Response shapes more than one tool advertises.
//!
//! One concept: the blocks that recur across verbs — a page, a proof, a
//! projection envelope, a quality report, warnings. A
//! shape used by a single verb belongs with that verb, not here; these earn
//! their place by having two or more.
//!
//! Depends only on `schema`, never on a tool.

use serde_json::{Value, json};

use super::primitives::{described, nullable_described, output_object, string_array};

pub(crate) fn warnings_output_schema() -> Value {
    string_array("Non-empty warnings qualify success; retain them.")
}
pub(crate) fn recall_envelope_properties() -> Value {
    json!({
        "projection": projection_output_schema(),
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
        "returned": described("integer", "Items returned on this page."),
        "total": described("integer", &format!("Total {unit} in the selection.")),
        "has_more": described(
            "boolean",
            "More items remain; this page is incomplete."
        ),
        "next_cursor": nullable_described("string", cursor_description)
    }))
}
pub(crate) fn relation_page_output_schema(unit: &str, cursor_description: &str) -> Value {
    let mut page = page_output_schema(unit, cursor_description);
    page["properties"]["offset"] = described(
        "integer",
        "Number of selected items already read before this page.",
    );
    page["properties"]["required_bytes"] = described(
        "integer",
        "When no whole item fits, retry page.next_cursor with budget.max_bytes at least this allowance to make progress. Absent otherwise.",
    );
    page
}

/// `proof`, which is where a caller decides whether to believe the answer.
pub(crate) fn proof_output_schema(confidence_description: &str) -> Value {
    output_object(json!({
        "confidence": described("string", confidence_description),
        "evidence": described(
            "array",
            "Verbatim stored text; metadata.proof_role distinguishes claim from evidence. \
             Retrieval metadata never changes that text. reached_by items are indirect proof, \
             never answers or citations in because; reached_from, reached_via and reached_hops \
             identify their route. On citations, bridged_terms names cross-language word pairs; \
             restated_from and restated_via identify a writer-declared restatement; matched_via: \
             summary and summary_terms identify words matched through the writer's English \
             summary_en, not the canonical text."
        ),
        "missing": described("array", "What was sought but not found: no retrieval or no evidence bearing on the question. Non-empty with UNKNOWN."),
        "superseded": described("array", "Replaced entries with superseded_by and why. Historical state, distinct from contradiction; not current advice."),
        "expired": described("array", "Entries past exclusive valid_until at the temporal cursor, recall as_of/interval end, or otherwise the memory's latest instant. Expiry needs no replacement."),
        "conflicts": described("array", "Explicit contradictions whose entries are both still live; distinct from supersession."),
        "matched_relations": described("array", "Typed relations contributing to ordering. Their prose may improve a match, never promote unrelated evidence into an answer."),
        "matched_terms": described("array", "Question terms matching retrieved evidence."),
        "path": described("array", "Traversal connecting cited evidence."),
        "frontier_size": described("integer", "Reachable items not returned; a signal to expand."),
        "interval": nullable_described("object", "Selected half-open [start,end) span, or null."),
        "axis": nullable_described("string", "Selected occurred/observed/ingested/validity clock, or default precedence; null at the memory's own frontier."),
        "as_of": nullable_described("string", "Selected instant, including the time resolved from a ref cursor; null if unrequested."),
        "nearest_outside": nullable_described("object", "On interval UNKNOWN, nearest match outside it: ref, time and axis. Distinguishes not then from not known; otherwise null."),
        "abouts_selected": described("array", "Abouts read under dimensions.scope, current one first. Refs retain ownership; this list distinguishes searched from unsearched abouts."),
        "abouts_empty_in_selection": described("array", "Searched abouts with no entry in the selected interval or effective at the selected instant; empty for an unbounded read.")
    }))
}
/// `projection`, the budget envelope on a recall.
fn projection_output_schema() -> Value {
    let mut page = page_output_schema(
        "eligible expansion items",
        "Opaque recall cursor for page.cursor, or null. Keep bound arguments unchanged; only page.entries, budget.tokens and budget.max_bytes may vary.",
    );
    page["properties"]["offset"] = described(
        "integer",
        "Number of eligible expansion items reconstructed by earlier pages.",
    );
    page["properties"]["minimum_progress_bytes"] = nullable_described(
        "integer",
        "When stalled or core_text_shortened, a sufficient byte allowance for the unshortened core and expansion progress. The returned action adds 10000 bytes for useful expansion instead of negotiating only one item.",
    );
    output_object(json!({
        "contract": described("string", "The projection contract version, e.g. kmp.recall.projection.v3."),
        "budget": described("object", "The normative byte ceiling, bytes actually used, and retained token-planning hint."),
        "detail": described("string", "compact | balanced | full — the detail tier that was served."),
        "excluded_by_detail": described(
            "integer",
            "Excluded by detail tier, not truncation; increase budget.detail to include them."
        ),
        "next_action": {
            "type":["object","null"],
            "description":"Execute this call unchanged. A continuation is {continuation}: a handle the server resolves to the complete call, cursor included. A restart when core_text_shortened is the complete call without page.cursor, recovering the full core. The proposed allowance is sufficient; keep the result partial if unavailable. Null when neither action remains.",
            "additionalProperties":false,
            "required":["tool","arguments"],
            "properties":{
                "tool":{"type":"string","enum":["kmp_ask","kmp_wake"]},
                "arguments":{"type":"object","description":"Either {continuation} or the complete bound request; both preserve question, original wording, clock, interval and dimension filters."}
            }
        },
        "page": page,
        "sections": described("object", "Per-section counts; a zero counter and a section with none are omitted. core and excluded_by_detail appear on the page that carries the core; returned_on_page and remaining on every page. remaining counts eligible expansion after this page, excluding the core and prior pages. eligible = core + all returned_on_page + last remaining; total = eligible + excluded_by_detail. Zero does not prove sufficient evidence; core_text_shortened, detail and selection caps still qualify coverage."),
        "selection_omitted": described("integer", "Items excluded by budget.max_entries before paging."),
        "core_text_shortened": described("boolean", "Whether stable core prose had to be shortened to fit max_bytes."),
        "core_reused": described("boolean", "Present and true on a continuation that omits the stable core: it carries only new expansion items. Combine them with the first page's core and earlier pages; page.repeat_core=true returns the core again.")
    }))
}
pub(crate) fn quality_output_schema() -> Value {
    output_object(json!({
        "nodes": described("integer", "Returned node count."),
        "relationships": described("integer", "Returned relation count."),
        "details": described("integer", "Returned node-detail count."),
        "causal_density": described(
            "number",
            "Share of returned relations that explain rather than merely connect; reflects the stored writing."
        ),
        "detail_coverage": described("number", "Share of returned nodes that carry stored detail."),
        "truncated": described("boolean", "Whether the rendering dropped anything.")
    }))
}
