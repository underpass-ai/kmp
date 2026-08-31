//! The bell, on tokio.

use std::future::Future;
use std::pin::Pin;

use tokio::sync::Notify;

use crate::view::ports::ChangeBell;

/// A [`ChangeBell`] over `tokio::sync::Notify`. Listeners are enabled before
/// they are handed out, so the port's race-free contract holds: a ring
/// landing between taking the listener and awaiting it still wakes it.
#[derive(Default)]
pub struct TokioChangeBell {
    notify: Notify,
}

impl TokioChangeBell {
    /// A quiet bell.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ChangeBell for TokioChangeBell {
    fn listen(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let mut listener = Box::pin(self.notify.notified());
        listener.as_mut().enable();
        listener
    }

    fn ring(&self) {
        self.notify.notify_waiters();
    }
}
