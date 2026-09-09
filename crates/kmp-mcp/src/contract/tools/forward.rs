use serde_json::Value;

use crate::contract::schema::temporal_family::temporal_tool_definition;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    temporal_tool_definition(
        "kmp_forward",
        "Move strictly after `from` (time, sequence or ref), oldest-to-newest. Execute next_actions unchanged to reconstruct entries and proof before continuing history. For [start,end), retain exact-start entries from Goto, merge Forward by ref and exclude entries at or after end.",
        "from",
    )
}
