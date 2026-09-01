//! A focus on the wire.

use serde::{Deserialize, Serialize};

use crate::view::application::dto::time_range_dto::TimeRangeDto;

/// A focus as the wire spells it: an optional window and the named refs.
///
/// A cleared window serializes as an explicit `null`, never an omission: a
/// full snapshot must tell the browser to drop a stale explicit range, which
/// is exactly what [#463](https://github.com/underpass-ai/kmp/issues/463)
/// caught it failing to do. An empty ref list stays omitted — the reader
/// already treats absence as emptiness there.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusDto {
    /// The framed stretch of time; `null` when nothing is framed.
    pub time_range: Option<TimeRangeDto>,
    /// The refs the focus names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
}
