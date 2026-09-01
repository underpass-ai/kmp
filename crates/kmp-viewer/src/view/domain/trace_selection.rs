//! The audit path drawn on the loom.

use crate::view::domain::memory_ref::MemoryRef;

/// A trace between two refs — "show me how this led to that". The path
/// itself is the kernel's answer; the view only remembers which two ends
/// were asked about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceSelection {
    /// Where the path starts.
    pub from: MemoryRef,
    /// Where the path ends.
    pub to: MemoryRef,
}
