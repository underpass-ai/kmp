use serde_json::Value;

use crate::contract::schema::temporal_family::time_tool_definition;

pub(crate) fn definition() -> Value {
    time_tool_definition(
        "Navigate an about's history on one clock (rewind, forward, goto, near): what changed, the latest, or the state at a date.",
    )
}
