//! Stepping back one change.

use crate::view::domain::{Actor, ViewError, ViewId, ViewState};
use crate::view::ports::{ChangeBell, SlotOutcome, ViewSessionStore, WallClock};

/// Undoes the last move on a view. Every visual action is reversible,
/// including the agent's — that is what makes handing it the wheel safe.
pub struct UndoViewMove<'a, Store, Bell, Clock> {
    /// Where sessions live.
    pub store: &'a Store,
    /// Rung when the undo moved the view.
    pub bell: &'a Bell,
    /// Stamps the attribution.
    pub wall_clock: &'a Clock,
}

impl<Store: ViewSessionStore, Bell: ChangeBell, Clock: WallClock>
    UndoViewMove<'_, Store, Bell, Clock>
{
    /// Executes one undo.
    pub fn execute(&self, view_id: Option<&str>, actor: &str) -> Result<ViewState, ViewError> {
        let id = ViewId::or_default(view_id);
        let actor = Actor::named(actor);
        let at = self.wall_clock.now();
        let state = self.store.operate(&id, |slot| match slot {
            None => SlotOutcome::answer(Err(ViewError::UnknownView(id.clone()))),
            Some(session) => SlotOutcome::answer(session.undo(actor, at)),
        })?;
        self.bell.ring();
        Ok(state)
    }
}
