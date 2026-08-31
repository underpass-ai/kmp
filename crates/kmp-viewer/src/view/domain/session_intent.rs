//! Everything one move needs to be judged.

use crate::view::domain::actor::Actor;
use crate::view::domain::idempotency_claim::IdempotencyClaim;
use crate::view::domain::timestamp::Timestamp;
use crate::view::domain::view_patch::ViewPatch;
use crate::view::domain::view_revision::ViewRevision;

/// One intent, ready for the aggregate: the change, the concurrency
/// expectation, the idempotency claim, and the attribution the move will
/// carry if it lands. Building one is the application's job; judging it is
/// the aggregate's.
#[derive(Clone, Debug)]
pub struct SessionIntent {
    /// The revision the caller saw, when they assert one.
    pub expected: Option<ViewRevision>,
    /// The intent's key and digest, when it travels under one.
    pub idempotency: Option<IdempotencyClaim>,
    /// The change itself.
    pub patch: ViewPatch,
    /// Who is moving the view.
    pub actor: Actor,
    /// Why, in the mover's words.
    pub explanation: Option<String>,
    /// When the move is landing.
    pub at: Timestamp,
}
