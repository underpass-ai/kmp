//! Where open sessions live.

use crate::view::domain::{ViewId, ViewSession, ViewState};

/// What one atomic operation decided: its result, and — when it decided to
/// open a session on a vacant id — the session to store.
pub struct SlotOutcome<R> {
    /// The operation's answer.
    pub result: R,
    /// A session to store under the id, when the operation opened one.
    pub open: Option<ViewSession>,
}

impl<R> SlotOutcome<R> {
    /// An outcome that only answers.
    pub fn answer(result: R) -> Self {
        Self { result, open: None }
    }

    /// An outcome that answers and opens a session.
    pub fn opens(result: R, session: ViewSession) -> Self {
        Self {
            result,
            open: Some(session),
        }
    }
}

/// Keeps the open view sessions. The store owns atomicity and lifetime — a
/// use case's whole judgment runs against a consistent session, and a view
/// nobody touches is eventually forgotten — while every decision about *what*
/// happens to a session stays in the domain.
pub trait ViewSessionStore: Send + Sync {
    /// Runs one atomic operation against the slot under `id`. The closure
    /// sees the open session, or `None` when the id is vacant; storing a
    /// newly opened session is part of the same atomic step.
    fn operate<R>(
        &self,
        id: &ViewId,
        operation: impl FnOnce(Option<&mut ViewSession>) -> SlotOutcome<R>,
    ) -> R;

    /// Reads the state under `id`, if a session is open there. Reading
    /// counts as touching: a view someone is watching does not expire under
    /// them.
    fn read(&self, id: &ViewId) -> Option<ViewState>;
}
