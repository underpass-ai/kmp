use crate::{AdjacencyPage, AdjacencyRequest, NodeProjection, PortError};

/// The coordinator borrows one consistent snapshot for its entire search.
/// Implementations must not open a new transaction per method call.
pub trait TraceSnapshotReader {
    /// Canonical bodies in requested order, preserving duplicates and missing slots.
    /// Reads must use the same snapshot as node/adjacency. N/E bound object work,
    /// not body allocation; a single canonical body may be arbitrarily large.
    fn bodies(
        &self,
        _ids: &[String],
    ) -> Result<Vec<Option<crate::NodeDetailProjection>>, PortError> {
        Err(PortError::Unavailable(
            "trace body batches are not supported by this snapshot".into(),
        ))
    }

    fn node(&self, id: &str) -> Result<Option<NodeProjection>, PortError>;
    fn adjacency(&self, request: &AdjacencyRequest) -> Result<AdjacencyPage, PortError>;
}
