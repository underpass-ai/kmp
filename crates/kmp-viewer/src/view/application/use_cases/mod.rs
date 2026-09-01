//! One operation per file. Each use case orchestrates ports and hands
//! judgment to the aggregate; none of them locks, sleeps, or serializes.

mod apply_view_intent;
mod await_view_change;
mod get_view_state;
mod open_view;
mod undo_view_move;

pub use apply_view_intent::ApplyViewIntent;
pub use await_view_change::AwaitViewChange;
pub use get_view_state::GetViewState;
pub use open_view::OpenView;
pub use undo_view_move::UndoViewMove;
