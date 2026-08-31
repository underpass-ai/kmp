//! The browser's long poll.

use std::time::Duration;

use crate::view::domain::{ViewId, ViewState};
use crate::view::ports::{ChangeBell, ViewSessionStore};

/// Resolves as soon as the view's revision moves past `since`, or when
/// patience runs out — so an agent's intent reaches the screen without
/// anyone polling in a loop.
pub struct AwaitViewChange<'a, Store, Bell> {
    /// Where sessions live.
    pub store: &'a Store,
    /// The bell that rings when any view moves.
    pub bell: &'a Bell,
}

impl<Store: ViewSessionStore, Bell: ChangeBell> AwaitViewChange<'_, Store, Bell> {
    /// Waits out one long poll.
    pub async fn execute(
        &self,
        view_id: Option<&str>,
        since: u64,
        patience: Duration,
    ) -> Option<ViewState> {
        let id = ViewId::or_default(view_id);
        let deadline = tokio::time::Instant::now() + patience;
        loop {
            // Listening starts before the state is read, on purpose: a
            // change that lands between the read and the await would
            // otherwise be missed and the caller would wait out its whole
            // patience for news that had already arrived.
            let waiting = self.bell.listen();
            let state = self.store.read(&id)?;
            if state.view_revision.value() > since {
                return Some(state);
            }
            if tokio::time::timeout_at(deadline, waiting).await.is_err() {
                return self.store.read(&id);
            }
        }
    }
}
