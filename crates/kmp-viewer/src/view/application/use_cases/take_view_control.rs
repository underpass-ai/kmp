//! Human handoff over the existing session, store and notification ports.

use crate::view::domain::{ViewError, ViewId, ViewRevision, ViewState};
use crate::view::ports::{ChangeBell, SlotOutcome, ViewSessionStore, WallClock};

/// Takes human control without changing the semantic frame or undo history.
pub struct TakeViewControl<'a, Store, Bell, Clock> {
    /// Where the shared session lives.
    pub store: &'a Store,
    /// Notifies the agent and other readers.
    pub bell: &'a Bell,
    /// Stamps provenance.
    pub wall_clock: &'a Clock,
}

impl<Store: ViewSessionStore, Bell: ChangeBell, Clock: WallClock>
    TakeViewControl<'_, Store, Bell, Clock>
{
    /// Refuses a stale handoff rather than claiming an unseen frame.
    pub fn execute(&self, view_id: Option<&str>, expected: u64) -> Result<ViewState, ViewError> {
        let id = ViewId::or_default(view_id);
        let at = self.wall_clock.now();
        let state = self.store.operate(&id, |slot| match slot {
            None => SlotOutcome::answer(Err(ViewError::UnknownView(id.clone()))),
            Some(session) => {
                SlotOutcome::answer(session.take_control(ViewRevision::from(expected), at))
            }
        })?;
        if state.view_revision.value() != expected {
            self.bell.ring();
        }
        Ok(state)
    }
}
