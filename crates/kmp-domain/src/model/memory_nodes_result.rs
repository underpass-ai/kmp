use crate::{MemoryNodeHeader, TraceSearchStop};

/// Deduplicated nodes in first-requested order from one operation snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryNodesResult {
    pub nodes: Vec<MemoryNodeHeader>,
    /// Adapter-certified identity; the pure resolver alone cannot certify reuse.
    pub snapshot: Option<String>,
    /// Absent or outside the requested about; foreign metadata is never exposed.
    pub missing: Vec<String>,
    /// References left unread by a common work budget, distinct from absence.
    pub omitted: Vec<String>,
    pub stop: Option<TraceSearchStop>,
    pub scanned_edges: u32,
}
