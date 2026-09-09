use serde_json::Value;

use crate::contract::schema::temporal_family::temporal_tool_definition;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    temporal_tool_definition(
        "kmp_rewind",
        "Read newest-to-oldest: start directly with interval [start,end), or move strictly before from (time, sequence or ref). Execute next_actions unchanged to reconstruct the packet and then continue earlier history within the same clock, dimensions and interval.",
        "from",
    )
}
