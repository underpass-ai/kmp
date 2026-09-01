//! A trace on the wire.

use serde::{Deserialize, Serialize};

/// A trace's two ends as the wire spells them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceSelectionDto {
    /// Where the path starts.
    pub from: String,
    /// Where the path ends.
    pub to: String,
}
