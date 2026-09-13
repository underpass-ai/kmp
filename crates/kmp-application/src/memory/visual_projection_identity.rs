use kmp_domain::GraphReadRevision;

use super::VisualProjectionQuery;

/// The complete request, without lossy hashing or normalized-away inputs.
/// The port revision certifies all graph, body and about-index reads and
/// distinguishes stores and reader incarnations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VisualProjectionIdentity {
    pub(super) revision: GraphReadRevision,
    pub(super) query: VisualProjectionQuery,
}
