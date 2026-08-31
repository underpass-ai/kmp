//! The bell that rings when a view moves.

use std::future::Future;
use std::pin::Pin;

/// Lets a change reach the screen without anyone polling in a loop. The
/// listener contract is race-free by construction: a listener returned by
/// [`ChangeBell::listen`] is already registered, so a ring landing between
/// taking the listener and awaiting it is not missed.
pub trait ChangeBell: Send + Sync {
    /// A listener, registered from the moment it is returned.
    fn listen(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;

    /// Rings the bell for everyone currently listening.
    fn ring(&self);
}
