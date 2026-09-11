use serde_json::{Value, json};

use super::primitives::{described, nullable_described, output_object};

pub(crate) fn write_clocks_schema() -> Value {
    let clock = output_object(json!({
        "entries": described("integer", "Memories with this clock, counted once across their memberships."),
        "distinct_values": described("integer", "Distinct instants carried by those memories."),
        "single_value": nullable_described("string", "RFC3339 instant when there is exactly one; null for none or several. Inspect receipt for individual clocks.")
    }));
    json!({"anyOf":[output_object(json!({
        "scope": described("string", "accepted_command: clocks of this historical write, including on replay; not current graph state or source fidelity."),
        "entries": described("integer", "Memories covered by this command."),
        "relations": {"anyOf": [output_object(json!({
            "relations": described("integer", "Semantic declarations in this command, excluding structural memberships."),
            "occurred": described("integer", "Relations with their own event time."),
            "observed": described("integer", "Relations with a declaration observation, independent of endpoint clocks."),
            "ingested": described("integer", "Relations with a stored ingestion time."),
            "valid_from": described("integer", "Relations with an explicit validity start."),
            "valid_until": described("integer", "Relations with an explicit validity end.")
        })), {"type":"null"}], "description":"Relation clock coverage; null means unavailable, not zero."},
        "occurred": clock, "observed": clock, "ingested": clock,
        "valid_from": clock, "valid_until": clock
    })), {"type":"null"}], "description":"Compact canonical clock coverage. Null when unavailable; previews do not report saved clocks."})
}
