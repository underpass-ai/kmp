use crate::{NodeBodyDescriptor, NodeCardExpectation, NodeCardStatus};

/// Orientation for a reader-authored card, bound to the selected body and card.
/// It neither contains prose nor establishes that a proof group is complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceCondenseCandidate {
    pub descriptor: NodeBodyDescriptor,
    pub card_status: NodeCardStatus,
    /// Selected routes, or structurally complete seek groups, using this object.
    pub shared_by: u32,
    pub expect: NodeCardExpectation,
}
