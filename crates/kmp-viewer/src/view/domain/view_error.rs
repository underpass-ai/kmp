//! Why an intent did not apply.

use crate::view::domain::idempotency_key::IdempotencyKey;
use crate::view::domain::view_id::ViewId;
use crate::view::domain::view_revision::ViewRevision;
use crate::view::domain::view_state::ViewState;

/// A conflict is not a failure of the agent; it is the human having moved.
/// It carries the current state so the caller can rebase without a second
/// round trip.
#[derive(Clone, Debug)]
pub enum ViewError {
    /// No view is open under that id.
    UnknownView(ViewId),
    /// The view moved past the revision the caller saw.
    Conflict {
        /// The revision the caller expected.
        expected: ViewRevision,
        /// The revision the view is actually at.
        actual: ViewRevision,
        /// The present state, for rebasing.
        current: Box<ViewState>,
    },
    /// The key was already honored for different content.
    IdempotencyConflict {
        /// The colliding key.
        key: IdempotencyKey,
    },
    /// The intent asked for something outside the domain's vocabulary or
    /// invariants, in words the caller can act on.
    Invalid(String),
}
