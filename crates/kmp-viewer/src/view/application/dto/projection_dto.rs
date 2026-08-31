//! Projection settings on the wire.

use serde::{Deserialize, Serialize};

/// The projection settings as the wire spells them — also the shape an
/// intent's `projection` block arrives in, before its vocabulary is checked
/// by the mapper.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionDto {
    /// The requested rung of the zoom ladder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_zoom: Option<String>,
    /// The dimensions to keep as lanes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<Vec<String>>,
    /// The relation classes to draw.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_classes: Option<Vec<String>>,
    /// Exact observability series to align over the current time window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlays: Option<Vec<String>>,
}
