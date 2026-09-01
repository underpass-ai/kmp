//! Opening a view, or rehydrating the one already under that id.

use crate::view::application::commands::OpenViewCommand;
use crate::view::domain::{
    AboutId, Actor, ViewError, ViewId, ViewRevision, ViewSession, ViewState,
};
use crate::view::ports::{ChangeBell, SlotOutcome, ViewSessionStore, WallClock};

/// Opens a view against the revision the caller actually saw. Changing the
/// about is a destructive camera reset, so unlike a same-about rehydrate it
/// is both concurrency-checked and attributed.
pub struct OpenView<'a, Store, Bell, Clock> {
    /// Where sessions live.
    pub store: &'a Store,
    /// Rung when the open moved the view.
    pub bell: &'a Bell,
    /// Stamps the attribution.
    pub wall_clock: &'a Clock,
}

impl<Store: ViewSessionStore, Bell: ChangeBell, Clock: WallClock> OpenView<'_, Store, Bell, Clock> {
    /// Executes one open.
    pub fn execute(&self, command: OpenViewCommand) -> Result<ViewState, ViewError> {
        let id = ViewId::or_default(command.view_id.as_deref());
        let about = command.about.map(AboutId::new);
        let expected = command.expected_revision.map(ViewRevision::from);
        let actor = Actor::named(&command.actor);
        let at = self.wall_clock.now();
        let (state, moved) = self.store.operate(&id, |slot| match slot {
            None => {
                // A vacant slot satisfies exactly one expectation: absence.
                if command
                    .expected_revision
                    .is_some_and(|expected| !ViewRevision::expects_absence(expected))
                {
                    return SlotOutcome::answer(Err(ViewError::UnknownView(id.clone())));
                }
                let session = ViewSession::opened(id.clone(), about.clone());
                let state = session.state().clone();
                SlotOutcome::opens(Ok((state, true)), session)
            }
            Some(session) => {
                let reopened = session.reopen(
                    about.clone(),
                    expected,
                    actor.clone(),
                    command.explanation.clone(),
                    at.clone(),
                );
                match reopened {
                    Ok(moved) => SlotOutcome::answer(Ok((session.state().clone(), moved))),
                    Err(error) => SlotOutcome::answer(Err(error)),
                }
            }
        })?;
        if moved {
            self.bell.ring();
        }
        Ok(state)
    }
}
