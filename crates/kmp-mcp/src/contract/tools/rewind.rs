use serde_json::Value;

use crate::contract::schema::temporal_family::temporal_tool_definition;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    temporal_tool_definition(
        "kmp_rewind",
        "Move backward through memory from a timestamp, sequence, or ref. Entries within each page are newest-to-oldest, so concatenating continuation pages stays globally descending. Cursor parameter: `from`.",
        "from",
    )
}
