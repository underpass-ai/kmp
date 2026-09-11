use crate::{NodeDetailProjection, NodeProjection};

/// One canonical object, shared by every selected path and support relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceProofObject {
    pub node: NodeProjection,
    pub body: Option<NodeDetailProjection>,
    pub coordinates: Vec<crate::TemporalCoordinate>,
}
