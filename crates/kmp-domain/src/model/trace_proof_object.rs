use crate::{NodeBodyDescriptor, NodeCardPresentation, NodeDetailProjection, NodeProjection};

/// One canonical object, shared by every selected path and support relation.
///
/// Body identity and body text are separate. `descriptor` is present whenever
/// the store holds a body and this read consulted descriptors at all, whether
/// or not the text came with it; `body` is present only when the text was
/// actually loaded. Everything else — ref, kind, source metadata, coordinates
/// — is unchanged by how the body was delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceProofObject {
    pub node: NodeProjection,
    /// Identity and size of the canonical body, without its text. `None` on
    /// the legacy unbounded read, which consults no descriptors, and when the
    /// store holds no body.
    pub descriptor: Option<NodeBodyDescriptor>,
    pub body: Option<NodeDetailProjection>,
    /// How this response delivered the body. Never means "a card exists".
    pub body_state: crate::TraceBodyState,
    /// What a compact presentation may show instead of the body. `None` when
    /// the read asked for no compact presentation; a card that cannot be shown
    /// never removes this object from the path.
    pub card: Option<NodeCardPresentation>,
    pub coordinates: Vec<crate::TemporalCoordinate>,
}

impl TraceProofObject {
    /// Exact stored bytes a named expansion of this one object would need.
    /// `None` when nothing is withheld or the store holds no body.
    pub fn required_record_bytes(&self) -> Option<u64> {
        (self.body.is_none())
            .then(|| self.descriptor.as_ref().map(|d| d.record_bytes))
            .flatten()
    }
}
