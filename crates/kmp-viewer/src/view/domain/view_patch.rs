//! One intent's worth of change.

use crate::view::domain::about_id::AboutId;
use crate::view::domain::clock::Clock;
use crate::view::domain::focus::Focus;
use crate::view::domain::focus_window::FocusWindow;
use crate::view::domain::memory_ref::MemoryRef;
use crate::view::domain::projection_settings::ProjectionSettings;
use crate::view::domain::search_query::SearchQuery;
use crate::view::domain::trace_selection::TraceSelection;

/// Every field is optional: an intent says what it means to change and stays
/// silent about the rest, so two agents editing different facets do not
/// clobber each other's. The `Option<Option<_>>` fields distinguish "leave
/// it alone" from "clear it" — the distinction #463 lives and dies on.
#[derive(Clone, Debug, Default)]
pub struct ViewPatch {
    /// Reopen the loom over this memory.
    pub about: Option<AboutId>,
    /// Read this axis.
    pub clock: Option<Clock>,
    /// Replaces the whole focus — what an intent means.
    pub focus: Option<Focus>,
    /// Moves only the window, leaving the focused refs alone — what a person
    /// panning means. Ignored when `focus` is present.
    pub focus_window: Option<FocusWindow>,
    /// Replaces the projection settings.
    pub projection: Option<ProjectionSettings>,
    /// `Some(None)` clears the selection; `None` leaves it alone.
    pub selection: Option<Option<MemoryRef>>,
    /// `Some(None)` clears the trace; `None` leaves it alone.
    pub trace: Option<Option<TraceSelection>>,
    /// `Some(None)` clears the search; `None` leaves it alone.
    pub search: Option<Option<SearchQuery>>,
}

impl ViewPatch {
    /// Whether the patch asks for anything at all. An empty patch is a read
    /// wearing a write's clothes, and is answered without moving the view.
    pub fn touches_anything(&self) -> bool {
        self.about.is_some()
            || self.clock.is_some()
            || self.focus.is_some()
            || self.focus_window.is_some()
            || self.projection.is_some()
            || self.selection.is_some()
            || self.trace.is_some()
            || self.search.is_some()
    }
}
