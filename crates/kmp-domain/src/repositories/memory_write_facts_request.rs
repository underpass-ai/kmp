use std::collections::BTreeSet;

/// The point reads one memory write asks of its about (DESIGN L6, write in
/// O(delta)): whether each named ref stands one hop from the about's anchor,
/// and the highest committed sequence of each named `(dimension, scope_id)`
/// coordinate. The about's dimensions are always answered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryWriteFactsRequest {
    pub about: String,
    pub refs: BTreeSet<String>,
    pub sequence_keys: BTreeSet<(String, String)>,
}

impl MemoryWriteFactsRequest {
    pub fn new(about: impl Into<String>) -> Self {
        Self {
            about: about.into(),
            ..Self::default()
        }
    }
}
