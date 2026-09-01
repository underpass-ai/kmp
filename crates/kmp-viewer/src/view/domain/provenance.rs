//! Who moved the view and why.

use crate::view::domain::actor::Actor;
use crate::view::domain::idempotency_key::IdempotencyKey;
use crate::view::domain::timestamp::Timestamp;

/// The attribution on a view change — shown to the human, so an agent can
/// never rearrange what they are looking at anonymously.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    /// Who moved it.
    pub actor: Actor,
    /// Why, in the mover's words, when they gave any.
    pub explanation: Option<String>,
    /// The intent's idempotency key, when the move came from one.
    pub idempotency_key: Option<IdempotencyKey>,
    /// When the move landed.
    pub at: Timestamp,
}
