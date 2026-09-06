//! What one intent asked for, as it arrived.

use serde::Serialize;

use crate::view::application::dto::focus_dto::FocusDto;
use crate::view::application::dto::label_selector_dto::LabelSelectorDto;
use crate::view::application::dto::projection_dto::ProjectionDto;
use crate::view::application::dto::time_range_dto::TimeRangeDto;
use crate::view::application::dto::trace_selection_dto::TraceSelectionDto;

/// The raw material of a view patch: every facet optional, vocabulary not
/// yet checked. Both boundaries build one — the MCP tools from a tool call,
/// the HTTP face from the browser's report — and its serialized form is the
/// intent's logical digest, taken before store-local names are resolved so a
/// retry remains the same intent even if the mounted catalog changed.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ViewIntentDto {
    /// Replace only the detail level; Some(None) selects automatic detail.
    pub projection_zoom: Option<Option<String>>,
    /// Replace only additional about planes; Some(None) clears them.
    pub projection_abouts: Option<Option<Vec<String>>>,
    /// Reopen the loom over this memory.
    pub about: Option<String>,
    /// Read this axis.
    pub clock: Option<String>,
    /// Replace the whole focus.
    pub focus: Option<FocusDto>,
    /// Move only the window, leaving focused refs alone. Ignored when
    /// `focus` is present.
    pub focus_window: Option<TimeRangeDto>,
    /// Replace the projection settings.
    pub projection: Option<ProjectionDto>,
    /// Replace only the label predicates, leaving the rest of the projection
    /// alone; `Some(None)` clears them. Ignored when `projection` is present.
    pub projection_labels: Option<Option<Vec<LabelSelectorDto>>>,
    /// `Some(None)` clears the selection; `None` leaves it alone.
    pub selection: Option<Option<String>>,
    /// `Some(None)` clears the trace; `None` leaves it alone.
    pub trace: Option<Option<TraceSelectionDto>>,
    /// `Some(None)` clears the search; `None` leaves it alone.
    pub search: Option<Option<String>>,
}
