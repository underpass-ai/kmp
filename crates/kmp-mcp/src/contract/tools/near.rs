use serde_json::Value;

use crate::contract::schema::temporal_family::temporal_tool_definition;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    temporal_tool_definition(
        "kmp_near",
        "Return the temporal neighborhood around `around` (timestamp, sequence or ref). Execute next_actions to reconstruct response pages; after the packet is complete, the actions offer kmp_rewind and kmp_forward for the surrounding history.",
        "around",
    )
}
