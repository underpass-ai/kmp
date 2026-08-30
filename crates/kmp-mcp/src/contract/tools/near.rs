use serde_json::Value;

use crate::contract::schema::temporal_family::temporal_tool_definition;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    temporal_tool_definition(
        "kmp_near",
        "Return the temporal neighborhood around a timestamp, sequence, or ref. Cursor parameter: `around`. A partial neighborhood continues through response.next_action with kmp_rewind and kmp_forward; feeding page.next_cursor back to kmp_near does not paginate.",
        "around",
    )
}
