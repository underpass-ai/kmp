use serde_json::Value;

use crate::contract::schema::temporal_family::temporal_tool_definition;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    temporal_tool_definition(
        "kmp_goto",
        "Jump to memory state at `at` (timestamp, sequence or ref). Follow executable next_actions: response pages reconstruct this packet first, then kmp_rewind explores earlier history. page.has_more counts packet expansion; selection.has_more reports history outside it.",
        "at",
    )
}
