use std::collections::{BTreeMap, BTreeSet};

/// What a store answered to a [`super::MemoryWriteFactsRequest`]: the same
/// facts a write reads off the about's depth-1 neighbourhood, read point by
/// point instead of materializing it.
///
/// - `exists`: the about's anchor exists (without it the about is new).
/// - `dimensions`: every `memory_dimension` node the anchor holds
///   (`has_dimension`), with its `dimension_kind` property when it has one.
/// - `present_refs` / `absent_refs`: of the refs asked, those that stand one
///   hop from the anchor as existing nodes, and those that do not.
/// - `sequence_frontiers`: for each coordinate asked, the highest sequence
///   its `contains_entry` edges hold (0 when none does).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryWriteFacts {
    pub exists: bool,
    pub dimensions: BTreeMap<String, Option<String>>,
    pub present_refs: BTreeSet<String>,
    pub absent_refs: BTreeSet<String>,
    pub sequence_frontiers: BTreeMap<(String, String), u32>,
}
