//! A time window on the wire.

use serde::{Deserialize, Serialize};

/// A focus window as the wire spells it. Either end may be absent.
///
/// An absent end is *omitted* from the serialized form today — the pinned
/// behavior [#463](https://github.com/underpass-ai/kmp/issues/463) will
/// change at this boundary and nowhere else.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRangeDto {
    /// Where the window opens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Where the window closes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}
