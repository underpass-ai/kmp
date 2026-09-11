use crate::{MemoryRelationType, RelationDirection};

/// An allowed move; direction never changes the stored assertion's arrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceRelationStep {
    pub relation: MemoryRelationType,
    pub direction: RelationDirection,
}
