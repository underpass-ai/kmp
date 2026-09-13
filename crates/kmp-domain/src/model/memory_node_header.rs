use crate::{NodeProjection, TemporalCoordinate};

/// One scoped node and all coordinates found within the batch's shared budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryNodeHeader {
    pub node: NodeProjection,
    pub coordinates: Vec<TemporalCoordinate>,
    pub coordinates_complete: bool,
}
