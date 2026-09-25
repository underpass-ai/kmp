//! One end of a path step.

use crate::view::domain::memory_ref::MemoryRef;

/// A fact a path passes through, with the about that owns it and the short
/// excerpt the path search read, so the loom can name a step whose fact is
/// outside the frame without reading it again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathEnd {
    /// The fact.
    pub reference: MemoryRef,
    /// The about that owns it.
    pub about: String,
    /// Its text as the path search excerpted it.
    pub excerpt: String,
}
