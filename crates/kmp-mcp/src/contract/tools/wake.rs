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
        "kmp_wake",
        "Return a compact Kernel Memory Protocol wake packet for continuing work from memory. `as_of`, `interval` and `axis` bound the packet in time the way they bound `kmp_ask`: its evidence, spine, resume cursor and proof stand on the selection, while the rendered summary is the about's.",
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
                "page": recall_page_schema(),
                "as_of": as_of_schema(),
                "interval": interval_schema(),
                "axis": recall_axis_schema()
            }
        }),
        wake_output_schema(),
    )
}

fn wake_output_schema() -> Value {
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
        "resume_cursor": nullable_described("object", "Newest temporal coordinate covered by this packet; null when the packet carries no temporal anchor."),
        "labels": described("array", "The catalogue of the abouts this wake read: every label their entries stand in, as `about`, `key` (the dimension kind), `value` (the scope id), `entries` and `last_observed_at`. The current about first, then by use. Read it before naming a label on a new memory, so an existing one is reused instead of guessed. The most used labels are the first expansion the packet fills; the rest follow the causal spine, and `truncation` reports what did not fit. Nothing of the core is shortened to make room for a label.")
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
