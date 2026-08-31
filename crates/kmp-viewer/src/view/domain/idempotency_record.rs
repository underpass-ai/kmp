//! What the aggregate remembers about an honored key.

use crate::view::domain::idempotency_claim::IdempotencyClaim;
use crate::view::domain::view_revision::ViewRevision;

/// One honored idempotency key: its claim, and the revision the aggregate
/// stood at when it was answered — so a replay can be answered with at least
/// that revision even after the view has moved on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdempotencyRecord {
    /// The claim as it was honored.
    pub claim: IdempotencyClaim,
    /// The revision the answer carried.
    pub revision: ViewRevision,
}
