use serde_json::Value;

use crate::contract::schema::temporal_family::temporal_tool_definition;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    temporal_tool_definition(
        "kmp_forward",
        "Read oldest-to-newest: start directly with interval [start,end), or move strictly after from (time, sequence or ref). Execute next_actions unchanged to reconstruct entries and proof, then continue within the same clock, dimensions and interval.",
        "from",
    )
}
