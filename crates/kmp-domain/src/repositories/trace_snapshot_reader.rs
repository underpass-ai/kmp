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

    /// Bodies checked against the digest their descriptor records, over the
    /// exact stored bytes, before they are returned.
    ///
    /// Separate from [`bodies`](Self::bodies) because the legacy unbounded
    /// read consults no descriptor and must keep working on a store written
    /// before descriptors existed. A read that binds cards or expansions to a
    /// digest asks for this one: delivering a body the descriptor does not
    /// describe would break every binding made against it.
    ///
    /// The default refuses rather than falling back to an unchecked read: a
    /// snapshot that cannot verify must not silently answer as if it had.
    fn verified_bodies(
        &self,
        _ids: &[String],
    ) -> Result<Vec<Option<crate::NodeDetailProjection>>, PortError> {
        Err(PortError::Unavailable(
            "verified body reads are not supported by this snapshot".into(),
        ))
    }

    /// Identity and size of each canonical body, in requested order,
    /// preserving duplicates and missing slots, from the same snapshot as
    /// `bodies`, `cards` and `adjacency`.
    ///
    /// `None` in a slot means the store holds no body for that ref. A stored
    /// body whose descriptor is missing is an inconsistent projection and must
    /// fail here: it is never reported as an absent body, and never licenses
    /// loading the whole record instead. No implementation may read the body
    /// value to answer this.
    fn descriptors(
        &self,
        _ids: &[String],
    ) -> Result<Vec<Option<crate::NodeBodyDescriptor>>, PortError> {
        Err(PortError::Unavailable(
            "body descriptors are not supported by this snapshot".into(),
        ))
    }

    /// Reader-authored cards in requested order, preserving duplicates and
    /// missing slots, from the same snapshot as bodies and adjacency. A
    /// snapshot that stores no cards answers every slot `None`, which reads as
    /// "no card", never as an error: a compact presentation that cannot find a
    /// card still returns its node.
    fn cards(
        &self,
        _ids: &[String],
        _language: &str,
    ) -> Result<Vec<Option<crate::NodeCard>>, PortError> {
        Ok(vec![None; _ids.len()])
    }

    fn node(&self, id: &str) -> Result<Option<NodeProjection>, PortError>;
    fn adjacency(&self, request: &AdjacencyRequest) -> Result<AdjacencyPage, PortError>;
}
