use serde_json::Value;

use crate::contract::schema::temporal_family::time_tool_definition;

pub(crate) fn definition() -> Value {
    time_tool_definition(
        "Navigate memory history on one clock. move: rewind reads newest first, strictly before `from` or within `interval`; forward reads oldest first, strictly after `from` or within `interval`; goto returns the state at `at`; near returns the neighbourhood around `around`. Cursors take time, sequence or ref. Execute next_actions unchanged: they finish this packet's response pages first, then continue history with the same clock, dimensions and interval. page.has_more counts packet expansion; selection.has_more reports history outside it.",
    )
}
