//! Applying one intent atomically.

use crate::view::application::Applied;
use crate::view::application::commands::ApplyIntentCommand;
use crate::view::application::mappers::{logical_digest, view_patch_from_intent};
use crate::view::domain::{
    Actor, IdempotencyClaim, IdempotencyKey, IntentDigest, SessionIntent, ViewError, ViewId,
    ViewRevision,
};
use crate::view::ports::{ChangeBell, SlotOutcome, ViewSessionStore, WallClock};

/// Applies one intent — focus, clock, filters, selection, trace — under
/// optimistic concurrency and idempotency. The vocabulary is refused before
/// the aggregate is reached, exactly as it always was; the aggregate judges
/// the rest.
pub struct ApplyViewIntent<'a, Store, Bell, Clock> {
    /// Where sessions live.
    pub store: &'a Store,
    /// Rung when the intent moved the view.
    pub bell: &'a Bell,
    /// Stamps the attribution.
    pub wall_clock: &'a Clock,
}

impl<Store: ViewSessionStore, Bell: ChangeBell, Clock: WallClock>
    ApplyViewIntent<'_, Store, Bell, Clock>
{
    /// Executes one intent.
    pub fn execute(&self, command: ApplyIntentCommand) -> Result<Applied, ViewError> {
        let patch = view_patch_from_intent(&command.intent)?;
        let id = ViewId::or_default(command.view_id.as_deref());
        let idempotency = command.idempotency_key.map(|key| IdempotencyClaim {
            key: IdempotencyKey::new(key),
            digest: command
                .intent_digest
                .map(IntentDigest::new)
                .unwrap_or_else(|| logical_digest(&command.intent)),
        });
        let intent = SessionIntent {
            expected: command.expected_revision.map(ViewRevision::from),
            idempotency,
            patch,
            actor: Actor::named(&command.actor),
            explanation: command.explanation,
            at: self.wall_clock.now(),
        };
        let outcome = self.store.operate(&id, |slot| match slot {
            None => SlotOutcome::answer(Err(ViewError::UnknownView(id.clone()))),
            Some(session) => SlotOutcome::answer(session.judge(intent)),
        })?;
        if outcome.applied {
            self.bell.ring();
        }
        Ok(Applied {
            state: outcome.state,
            applied: outcome.applied,
            unhonored: command.unhonored,
        })
    }
}
