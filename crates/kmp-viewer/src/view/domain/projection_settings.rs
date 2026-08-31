//! How the loom renders what the focus frames.

use crate::view::domain::dimension_name::DimensionName;
use crate::view::domain::overlay_name::OverlayName;
use crate::view::domain::relation_class::RelationClass;
use crate::view::domain::semantic_zoom::SemanticZoom;

/// The projection settings on a view. Each field is a keep-list: `None`
/// means "everything", which is why replacing the settings with an intent's
/// explicit lists is meaningful.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectionSettings {
    /// The requested rung of the semantic-zoom ladder.
    pub semantic_zoom: Option<SemanticZoom>,
    /// The dimensions to keep as lanes.
    pub dimensions: Option<Vec<DimensionName>>,
    /// The relation classes to draw.
    pub relation_classes: Option<Vec<RelationClass>>,
    /// Exact observability series to align over the current time window.
    pub overlays: Option<Vec<OverlayName>>,
}
