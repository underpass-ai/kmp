use crate::{AdjacencyPage, AdjacencyRequest, NodeProjection, PortError};

/// The coordinator borrows one consistent snapshot for its entire search.
/// Implementations must not open a new transaction per method call.
pub trait TraceSnapshotReader {
    fn node(&self, id: &str) -> Result<Option<NodeProjection>, PortError>;
    fn adjacency(&self, request: &AdjacencyRequest) -> Result<AdjacencyPage, PortError>;
}
