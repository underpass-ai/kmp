use serde_json::Value;

use crate::contract::schema::temporal_family::temporal_tool_definition;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    temporal_tool_definition(
        "kmp_rewind",
        "Move strictly before `from` (time, sequence or ref), newest-to-oldest. Execute next_actions unchanged to reconstruct the packet and then continue earlier history. page.has_more counts entry/proof expansion; selection.has_more reports history outside the packet.",
        "from",
    )
}
