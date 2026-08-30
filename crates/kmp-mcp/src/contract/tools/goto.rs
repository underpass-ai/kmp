use serde_json::Value;

use crate::contract::schema::temporal_family::temporal_tool_definition;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    temporal_tool_definition(
        "kmp_goto",
        "Jump to memory state at a timestamp, sequence, or ref. Cursor parameter: `at`. When the result is partial, response.next_action continues earlier history with kmp_rewind; feeding page.next_cursor back to kmp_goto does not paginate.",
        "at",
    )
}
