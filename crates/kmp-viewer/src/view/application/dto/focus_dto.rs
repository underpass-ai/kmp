//! A focus on the wire.

use serde::{Deserialize, Serialize};

use crate::view::application::dto::time_range_dto::TimeRangeDto;

/// A focus as the wire spells it: an optional window and the named refs.
/// An absent window is omitted and an empty ref list is omitted — today's
/// pinned bytes, until the [#463](https://github.com/underpass-ai/kmp/issues/463)
/// fix makes cleared facets explicit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusDto {
    /// The framed stretch of time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range: Option<TimeRangeDto>,
    /// The refs the focus names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
}
