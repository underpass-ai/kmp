//! `kmp_ask` — retrieval with proof, or UNKNOWN.
//!
//! One concept: what asking accepts and the evidence envelope it answers with.

use serde_json::{Value, json};

use crate::protocol::request_shape::{budget_schema, dimensions_schema, recall_page_schema};
use crate::protocol::response_shape::{proof_output_schema, recall_envelope_properties};
use crate::protocol::schema::{
    described, integer_schema, nullable_described, output_object, string_schema,
};

pub(in crate::protocol) fn definition() -> Value {
    super::definition_with_output(
        "kmp_ask",
        "Retrieve stored entry text and evidence bearing on a question, or UNKNOWN. Nothing is generated: `answer` names what was retrieved and the text lives in `proof.evidence[].text` — `metadata.proof_role` distinguishes an entry claim from stored evidence. Read it and judge whether it answers. `proof.confidence` is lexical term overlap between the question and the best-matching memory item; it is not a judgement that the item answers, and it is not the `confidence` on a relation, which is writer certainty. UNKNOWN means memory did not answer; `summary` says whether nothing was retrieved or nothing retrieved bore on the question.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["about", "question"],
            "properties": {
                "about": string_schema("Memory anchor or root ref to ask from."),
                "question": string_schema("Natural-language question."),
                "answer_policy": {
                    "type": "string",
                    "description": "Deterministic evidence policy. show_conflicts surfaces explicit conflict relations in proof.conflicts; best_effort does not generate fallback text.",
                    "enum": ["evidence_or_unknown", "show_conflicts", "best_effort"]
                },
                "dimensions": dimensions_schema(),
                "depth": integer_schema("Optional graph traversal depth. Applies in embedded and live gRPC modes; it overrides budget.depth."),
                "budget": budget_schema(2_400, 2),
                "page": recall_page_schema()
            }
        }),
        output_schema(),
    )
}

pub(in crate::protocol) fn output_schema() -> Value {
    let mut properties = json!({
        "summary": described("string", "States whether nothing was retrieved, retrieved evidence did not bear on the question, or evidence was retained."),
        "answer": nullable_described("string", "UNKNOWN when the selected policy found no answerable evidence; otherwise names the retrieved citations without claiming they prove the answer."),
        "because": described("array", "At most five retained citation refs. Empty beside UNKNOWN; canonical text lives in proof.evidence."),
        "proof": proof_output_schema("Derived from lexical term overlap between the question and the best retained evidence item. It is not a judgement that the evidence answers the question, and it is not relation-writer certainty.")
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
