use std::collections::BTreeMap;

use super::ExistingMemoryRefs;

/// A write's reading of its about taken point by point (DESIGN L6, write in
/// O(delta)): what its translation asks, the about revision read before
/// it, and the frontier of every coordinate the write leaves its sequence
/// to the kernel in, as read (0 when the scope holds none).
#[derive(Debug, Clone)]
pub(super) struct PointedWrite {
    pub(super) existing: ExistingMemoryRefs,
    pub(super) revision: u64,
    pub(super) frontiers: BTreeMap<(String, String), u32>,
}
