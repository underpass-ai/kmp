use serde_json::{Value, json};

use crate::contract::schema::definition::tool_definition_with_output;
#[allow(unused_imports)]
use crate::contract::schema::paging::*;
#[allow(unused_imports)]
use crate::contract::schema::primitives::*;
#[allow(unused_imports)]
use crate::contract::schema::request_shape::*;
#[allow(unused_imports)]
use crate::contract::schema::response_shape::*;

pub(crate) fn definition() -> Value {
    tool_definition_with_output(
        "kmp_summaries_audit",
        "Read where this store's memories stand with respect to their English search summaries, so the one writer that can produce a summary can see what to repair. Deterministic and repeatable: it reads the store's own event log, generates no text, changes nothing and never rejects a memory. Each entry returns a state — missing, refused, stands, not_required — with the lint's own faults when a summary is refused, and deterministic weakness signals when one stands and still retrieves little. Both feed kmp_write_memory search_summaries unchanged: render the text again and attach it. Scope defaults to one about; dimensions.scope abouts reads a named set and all_abouts sweeps every anchor, at a real cost. Page with page.cursor and the same bound arguments; totals describe the whole selection, not the page. It judges form, never meaning: a fluent, wrong summary passes, exactly as the lint does.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "about": string_schema("The about to audit. Required for the default scope; omit it only with dimensions.scope all_abouts, which names no single anchor."),
                "dimensions": audit_scope_schema(),
                "states": {
                    "type": "array",
                    "minItems": 1,
                    "uniqueItems": true,
                    "description": "Keep only these states. Omitted: every memory of the selection, which is what an audit is. Narrow to missing and refused for the debt alone, or add stands to read the weakness signals with it. Totals always describe the unfiltered selection, so a narrowed read still says how much of the about it covered.",
                    "items": {"type": "string", "enum": ["missing", "refused", "stands", "not_required"]}
                },
                "budget": summaries_audit_budget_schema(),
                "page": page_schema("Optional maximum memories on this page; the byte ceiling still applies, and a page always carries at least one memory so a continuation cannot stall.")
            }
        }),
        summaries_audit_output_schema(),
    )
}

/// The scope vocabulary the reads already use, narrowed to what this reading
/// honours. A label predicate is deliberately absent: this is read off the
/// event log, which carries no label projection, and advertising a filter
/// that is silently ignored is worse than not offering one.
fn audit_scope_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Which abouts this reading covers, in the same words kmp_ask reads several abouts together.",
        "properties": {
            "scope": {
                "type": "string",
                "description": "Default current_about audits the named about alone. abouts reads the list in dimensions.abouts. all_abouts sweeps every anchor and is the explicit opt-in to that cost on a large store; it is never a default.",
                "enum": ["current_about", "abouts", "all_abouts"]
            },
            "abouts": {
                "type": "array",
                "minItems": 1,
                "description": "Non-empty selection for scope=abouts. Include the current about if wanted.",
                "items": string_schema("Memory about id.")
            }
        }
    })
}

fn summaries_audit_budget_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "max_bytes": {
                "type": "integer",
                "minimum": 512,
                "default": 10_000,
                "description": "Compact-JSON structuredContent byte ceiling. The totals and the per-about counts are the stable core; the memories page under it. One memory larger than the whole ceiling is still returned, with a warning, so a continuation always advances."
            }
        }
    })
}

fn summaries_audit_output_schema() -> Value {
    output_object(json!({
        "summary": described("string", "What this reading covered and how much of it owes a summary."),
        "scope": output_object(json!({
            "selection": described("string", "current_about, abouts or all_abouts — the scope this reading applied."),
            "abouts": string_array("The abouts the selection named. Empty for all_abouts, which names none.")
        })),
        "totals": summaries_audit_totals_schema("Counts over the whole selection, before any states filter and independent of paging. The doctor prints these, from this same reading."),
        "abouts": {
            "type": "array",
            "description": "The same counts per about, in the order the store met them.",
            "items": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "about": described("string", "The about these counts are for."),
                    "totals": summaries_audit_totals_schema("Counts over this about's memories.")
                }
            }
        },
        "entries": {
            "type": "array",
            "description": "One memory per item, in the order the store met them, filtered by states and cut by the page.",
            "items": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "about": described("string", "The about that owns this memory."),
                    "ref": described("string", "The memory ref. Pass it back in kmp_write_memory search_summaries[].ref, byte for byte."),
                    "kind": described("string", "What the memory records."),
                    "state": described("string", "missing: no summary and an English question cannot reach the text. refused: a summary the lint will not accept, so it carries no retrieval. stands: the lint accepts it. not_required: no summary and none is needed, because the text is already in the kernel's search language."),
                    "text": described("string", "The stored text, byte for byte, present only when this memory needs a rendering written — a debt, or a summary that stands weakly. Render this; never alter it to fit a summary."),
                    "summary_en": described("string", "The stored English rendering, when there is one."),
                    "summary_en_by": described("string", "Who wrote that rendering, when the store knows. This is what makes \"regenerate everything an older writer produced\" answerable."),
                    "faults": string_array("Why the lint refuses this summary, in the lint's own words — the same sentences kmp_ingest and kmp_write_memory put in front of a writer. Present only in the refused state."),
                    "weaknesses": {
                        "type": "array",
                        "description": "Deterministic signals that this accepted summary still carries little retrieval. Warnings, never refusals: the memory is intact and the fix is a better rendering.",
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "signal": described("string", "thin: it clears the lint's floor and little more, for a much longer text. repeated: other memories of this about carry it word for word. undiscriminating: every informative word of it already reaches most of this about's other memories. stale: the text was rewritten after the summary was last written."),
                                "says": described("string", "What to change, in one sentence.")
                            }
                        }
                    }
                }
            }
        },
        "page": summaries_audit_page_schema(),
        "next_actions": {
            "type": "array",
            "description": "The complete call that continues this reading. Execute it and retain the earlier pages; empty when the reading is complete.",
            "items": output_object(json!({
                "tool": {"type": "string", "const": "kmp_summaries_audit"},
                "arguments": described("object", "The complete call, with every bound argument preserved and page.cursor set.")
            }))
        },
        "warnings": warnings_output_schema()
    }))
}

fn summaries_audit_totals_schema(description: &str) -> Value {
    let mut totals = output_object(json!({
        "entries": described("integer", "Memories read."),
        "missing": described("integer", "With no summary, whose text an English question cannot reach."),
        "refused": described("integer", "Carrying a summary the lint refuses."),
        "stands": described("integer", "Carrying a summary the lint accepts."),
        "not_required": described("integer", "With no summary and no need of one."),
        "weak": described("integer", "Of those that stand, how many carry at least one weakness signal. Never counted as owing a summary."),
        "owed": described("integer", "missing plus refused: the memories `kmp-mcp summaries pending` lists and the doctor counts.")
    }));
    totals["description"] = json!(description);
    totals
}

fn summaries_audit_page_schema() -> Value {
    let mut page = page_output_schema(
        "memories in the selection after the states filter",
        "Opaque audit cursor for page.cursor, or null. Repeat every bound argument unchanged; only page.entries and budget.max_bytes may vary. A changed selection rejects the cursor rather than paging a different reading.",
    );
    page["properties"]["offset"] = described(
        "integer",
        "Memories already read by earlier pages of this continuation.",
    );
    page["properties"]["required_bytes"] = described(
        "integer",
        "Present only when one memory alone exceeded the byte ceiling and was returned anyway. Raise budget.max_bytes to this to read the page without that overrun.",
    );
    page
}
