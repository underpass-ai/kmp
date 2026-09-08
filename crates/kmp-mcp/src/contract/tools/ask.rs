use serde_json::{Value, json};

use crate::contract::schema::definition::tool_definition_with_output;
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
        "kmp_ask",
        "Retrieve cited stored text and evidence, or UNKNOWN; never generate an answer. Read proof.evidence[].text and judge whether it answers. A dated semantic question uses as_of or a half-open UTC interval and axis; proof records the selection and nearest_outside on a bounded UNKNOWN. If the first English question finds UNKNOWN or irrelevant evidence, re-ask at most once in the user's own words. Changing optional arguments is another selection, not pagination; continue only through projection.page.next_cursor with bound arguments unchanged. After those selections, reclassify the original goal: history/current state uses temporal navigation; genuinely semantic UNKNOWN is terminal. Do not inspect the root, widen scope or traverse the graph to bypass it. proof.confidence measures lexical overlap, not answer correctness or relation-writer certainty; reached_by items are supporting proof, not direct answers. See proof fields for language-bridge and summary provenance.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["about", "question"],
            "properties": {
                "about": string_schema("Memory anchor or root ref to ask from."),
                "question": string_schema("Natural-language question, in the kernel's search language: plain English, with every number, identifier and acronym the user wrote kept exactly. The kernel searches it as given and never translates it. It accepts any language — a question in the store's own language reaches the stored text directly — and a memory written in another language is reached through its English `summary_en`."),
                "asked_as": string_schema("The user's own words when `question` renders them in English. Optional. Searched never; echoed back as `asked_as` for the audit trail, and read against `question` the way a search summary is read against its text: a rendering that leans to another language, carries no informative word, or drops an identifier the user's words carry (`#469`, `v0.7.0`) draws a warning, while the question is still searched as given. Answer in the user's language; cite the stored text byte for byte."),
                "answer_policy": {
                    "type": "string",
                    "description": "Deterministic evidence policy. Explicit `contradicts` relations on the proof path are surfaced in proof.conflicts under every policy. evidence_or_unknown and show_conflicts return UNKNOWN when the best retained evidence barely shares a term with the question; show_conflicts selects no additional conflict detection today. best_effort keeps that low-overlap neighbourhood and never generates fallback text.",
                    "enum": ["evidence_or_unknown", "show_conflicts", "best_effort"]
                },
                "dimensions": dimensions_schema(),
                "depth": integer_schema("Optional graph traversal depth. Applies in embedded and live gRPC modes; it overrides budget.depth."),
                "budget": budget_schema(2_400, 2),
                "page": recall_page_schema(),
                "as_of": as_of_schema(),
                "interval": interval_schema(),
                "axis": recall_axis_schema()
            }
        }),
        ask_output_schema(),
    )
}

fn ask_output_schema() -> Value {
    let mut properties = json!({
        "summary": described("string", "States whether nothing was retrieved, retrieved evidence did not bear on the question, or evidence was retained."),
        "answer": nullable_described("string", "UNKNOWN when the selected policy found no answerable evidence; otherwise names the retrieved citations without claiming they prove the answer."),
        "because": described("array", "At most five retained citation refs. Empty beside UNKNOWN; canonical text lives in proof.evidence."),
        "asked_as": described("string", "The user's own words, echoed byte for byte when the request carried them. Absent otherwise."),
        "proof": proof_output_schema("Derived from lexical term overlap between the question and the best retained evidence item. It is not a judgement that the evidence answers the question, and it is not relation-writer certainty. Medium at most when the question reached the evidence through the lexical-bridge table rather than in its own words; not capped when the writer's English summary_en carried it.")
    });
    properties
        .as_object_mut()
        .expect("output properties")
        .extend(
            recall_envelope_properties()
                .as_object()
                .expect("envelope properties")
                .clone(),
        );
    output_object(properties)
}
