//! What one judged move produced.

use crate::view::domain::view_state::ViewState;

/// The aggregate's answer to one intent: the state to hand back, and whether
/// this call was the one that moved it. A replayed key and a no-op both come
/// back honest — `applied` is false, and nothing needs re-rendering.
#[derive(Clone, Debug)]
pub struct SessionOutcome {
    /// The state after judging the intent.
    pub state: ViewState,
    /// Whether this call moved the view.
    pub applied: bool,
}
