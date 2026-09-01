//! A time window on the wire.

use serde::{Deserialize, Serialize};

/// A focus window as the wire spells it. Either end may be absent, and an
/// absent end is omitted: an open end is not a cleared facet, so the
/// [#463](https://github.com/underpass-ai/kmp/issues/463) explicit-null rule
/// applies to the window as a whole, not to its ends.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRangeDto {
    /// Where the window opens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Where the window closes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}
