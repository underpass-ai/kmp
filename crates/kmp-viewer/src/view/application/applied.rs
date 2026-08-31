//! The application's answer to an intent.

use crate::view::domain::ViewState;

/// What applying an intent produced: the new state, whether this call was
/// the one that produced it (a replayed idempotency key is honest about it),
/// and the parts of the intent the mounted store could not honor by name.
#[derive(Clone, Debug)]
pub struct Applied {
    /// The state after the intent.
    pub state: ViewState,
    /// Whether this call moved the view.
    pub applied: bool,
    /// The parts of the intent the boundary could not honor, in the
    /// caller's words — unresolvable names and priority notes alike. They
    /// are reported rather than drawn as if they were data, and they are
    /// boundary prose, which is why this result lives in the application
    /// ring and not in the domain.
    pub unhonored: Vec<String>,
}
