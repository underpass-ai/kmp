use serde_json::Value;

use crate::contract::schema::temporal_family::temporal_tool_definition;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    temporal_tool_definition(
        "kmp_forward",
        "Move forward through memory from a timestamp, sequence, or ref. Entries and continuation pages are oldest-to-newest. Cursor parameter: `from`.",
        "from",
    )
}
