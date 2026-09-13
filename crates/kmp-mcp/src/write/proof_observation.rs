//! The observation a declaration carries onto the proof it generates.
//!
//! One concept: a semantic link and the evidence compiled beside it are
//! asserted *when the writer says they were asserted*, never when one of
//! their endpoints was observed. An old source linked today is linked today.

use serde_json::{Value, json};

/// The clock object a relation and its evidence carry. An empty object opts
/// into the command's ingestion default without borrowing the packet's or a
/// target's observation.
pub(crate) fn proof_clocks(observed_at: Option<&Value>) -> Value {
    let mut clocks = json!({});
    if let Some(observed) = observed_at.filter(|value| !value.is_null()) {
        clocks["observed_at"] = observed.clone();
    }
    clocks
}

/// A member declares its proof at its effective observation. Preserve that
/// provenance before merging members into one packet.
pub(crate) fn preserve_proof_observation(arguments: &mut Value) {
    let clocks = proof_clocks(arguments.pointer("/provenance/observed_at"));
    for relation in arguments["memory"]["relations"]
        .as_array_mut()
        .expect("compiled relations")
    {
        if relation["class"] != "structural" {
            relation["clocks"] = clocks.clone();
        }
    }
    for evidence in arguments["memory"]["evidence"]
        .as_array_mut()
        .expect("compiled evidence")
    {
        evidence["support_clocks"] = clocks.clone();
    }
}
