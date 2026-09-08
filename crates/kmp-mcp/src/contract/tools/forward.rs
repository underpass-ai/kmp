use serde_json::Value;

use crate::contract::schema::temporal_family::temporal_tool_definition;
#[allow(clippy::unused_unit)]
pub(crate) fn definition() -> Value {
    temporal_tool_definition(
        "kmp_forward",
        "Move strictly after `from` (time, sequence or ref), oldest-to-newest. To enumerate [start,end), first kmp_goto at start and retain entries exactly at start; then kmp_forward from start. Complete relevant pages, merge and deduplicate refs, and exclude entries at or after end. Follow next_action for continuation; disclose any incomplete boundary or interval.",
        "from",
    )
}
