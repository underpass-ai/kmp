//! The state both faces of the loom read.

use crate::view::domain::about_id::AboutId;
use crate::view::domain::clock::Clock;
use crate::view::domain::focus::Focus;
use crate::view::domain::memory_ref::MemoryRef;
use crate::view::domain::projection_settings::ProjectionSettings;
use crate::view::domain::provenance::Provenance;
use crate::view::domain::search_query::SearchQuery;
use crate::view::domain::trace_selection::TraceSelection;
use crate::view::domain::view_id::ViewId;
use crate::view::domain::view_patch::ViewPatch;
use crate::view::domain::view_revision::ViewRevision;

/// One view's semantic state: which memory, which clock, what is framed,
/// filtered, selected and traced — and who last moved it. This is the whole
/// truth the browser renders and the agent reads back; there is no second,
/// richer state hiding behind it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewState {
    /// Which loom this is.
    pub view_id: ViewId,
    /// The aggregate's own counter.
    pub view_revision: ViewRevision,
    /// The memory the loom is woven over.
    pub about: Option<AboutId>,
    /// The axis the loom reads.
    pub clock: Clock,
    /// What the view is framed on.
    pub focus: Focus,
    /// How the frame is rendered.
    pub projection: ProjectionSettings,
    /// The selected entry, when one is selected.
    pub selection: Option<MemoryRef>,
    /// The drawn audit path, when one is drawn.
    pub trace: Option<TraceSelection>,
    /// The search filter, when one is set.
    pub search: Option<SearchQuery>,
    /// Who last moved the view.
    pub last_change: Option<Provenance>,
    /// Whether a move remains to step back from.
    pub can_undo: bool,
}

impl ViewState {
    /// A freshly opened view: revision one, the default clock, nothing
    /// framed, filtered or selected.
    pub fn opened(view_id: ViewId, about: Option<AboutId>) -> Self {
        Self {
            view_id,
            view_revision: ViewRevision::initial(),
            about,
            clock: Clock::default(),
            focus: Focus::default(),
            projection: ProjectionSettings::default(),
            selection: None,
            trace: None,
            search: None,
            last_change: None,
            can_undo: false,
        }
    }

    /// Applies one patch in place. Every field the patch leaves alone
    /// survives untouched; a field it sets to "nothing" is cleared, which is
    /// different from being left alone.
    pub fn apply(&mut self, patch: ViewPatch) {
        if let Some(about) = patch.about {
            self.about = Some(about);
        }
        if let Some(clock) = patch.clock {
            self.clock = clock;
        }
        if let Some(focus) = patch.focus {
            self.focus = focus;
        } else if let Some(window) = patch.focus_window {
            self.focus.window = Some(window);
        }
        if let Some(projection) = patch.projection {
            self.projection = projection;
        }
        if let Some(selection) = patch.selection {
            self.selection = selection;
        }
        if let Some(trace) = patch.trace {
            self.trace = trace;
        }
        if let Some(search) = patch.search {
            self.search = search;
        }
    }
}
