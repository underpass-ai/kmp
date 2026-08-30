//! `kmp_wake` — the continuation packet.
//!
//! One concept: what waking an about accepts and the packet it answers with.

use serde_json::{Value, json};

use crate::protocol::request_shape::{budget_schema, dimensions_schema, recall_page_schema};
use crate::protocol::response_shape::{proof_output_schema, recall_envelope_properties};
use crate::protocol::schema::{
    described, integer_schema, nullable_described, output_object, string_array, string_schema,
};

pub(in crate::protocol) fn definition() -> Value {
    super::definition_with_output(
        "kmp_wake",
        "Return a compact Kernel Memory Protocol wake packet for continuing work from memory.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["about"],
            "properties": {
                "about": string_schema("Memory anchor or root ref to wake from."),
                "role": string_schema("Optional caller role."),
                "intent": string_schema("Optional continuation intent."),
                "dimensions": dimensions_schema(),
                "depth": integer_schema("Optional graph traversal depth. Applies in embedded and live gRPC modes; it overrides budget.depth."),
                "budget": budget_schema(1_600, 2),
                "page": recall_page_schema()
            }
        }),
        output_schema(),
    )
}

pub(in crate::protocol) fn output_schema() -> Value {
    let mut properties = json!({
        "summary": described("string", "Four-line L0 resume summary: objective, status, blocker, and next action."),
        "wake": output_object(json!({
            "objective": described("string", "The continuation intent supplied by the caller."),
            "current_state": string_array("Current semantic state selected from the rendered memory."),
            "causal_spine": described("array", "Highest-salience explanatory relations, each with claim, because, and evidence_ref."),
            "open_loops": string_array("Live blocker statements reflected by the L0 summary; empty means no blocker was identified."),
            "next_actions": string_array("Next-action statements reflected by the L0 summary; empty means no concrete next action was identified."),
            "guardrails": string_array("Stored constraints the continuation should preserve.")
        })),
        "proof": proof_output_schema("Wake uses medium for a non-empty deterministic retrieval packet; this is not relation-writer certainty."),
        "resume_cursor": nullable_described("object", "Newest temporal coordinate covered by this packet; null when the packet carries no temporal anchor.")
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
