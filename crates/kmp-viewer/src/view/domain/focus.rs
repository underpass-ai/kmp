//! What the view is framed on.

use crate::view::domain::focus_window::FocusWindow;
use crate::view::domain::memory_ref::MemoryRef;

/// The focus of a view: a stretch of time, a set of refs, or both. An intent
/// replaces the whole focus — that is what "focus these" means — while a
/// person panning moves only the window and leaves the refs alone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Focus {
    /// The framed stretch of time, when one is framed.
    pub window: Option<FocusWindow>,
    /// The refs the focus names, in the order they were named.
    pub refs: Vec<MemoryRef>,
}
