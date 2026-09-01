//! The full view snapshot on the wire.

use serde::{Deserialize, Serialize};

use crate::view::application::dto::focus_dto::FocusDto;
use crate::view::application::dto::projection_dto::ProjectionDto;
use crate::view::application::dto::provenance_dto::ProvenanceDto;
use crate::view::application::dto::trace_selection_dto::TraceSelectionDto;

/// One view's semantic state as both faces receive it. Field order and every
/// `skip_serializing_if` are wire contract: what serializes here is what the
/// browser's long poll and the MCP tools' `state` carry.
///
/// A cleared search serializes as an explicit `null`, never an omission — a
/// full snapshot must deterministically reconcile the browser, and an
/// omitted field is indistinguishable from "leave your stale value alone"
/// ([#463](https://github.com/underpass-ai/kmp/issues/463)).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewStateDto {
    /// Which loom this is.
    pub view_id: String,
    /// The aggregate's own counter.
    pub view_revision: u64,
    /// The memory the loom is woven over.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    /// The axis the loom reads.
    pub clock: String,
    /// What the view is framed on.
    pub focus: FocusDto,
    /// How the frame is rendered.
    pub projection: ProjectionDto,
    /// The selected entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<String>,
    /// The drawn audit path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<TraceSelectionDto>,
    /// The search filter; `null` when none is set.
    pub search: Option<String>,
    /// Who last moved the view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_change: Option<ProvenanceDto>,
    /// Whether a move remains to step back from.
    pub can_undo: bool,
}
