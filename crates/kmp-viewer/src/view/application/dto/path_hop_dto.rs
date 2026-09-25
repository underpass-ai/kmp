//! A path step on the wire.

use serde::{Deserialize, Serialize};

use crate::view::application::dto::path_end_dto::PathEndDto;

/// One hop of a chain, spelled the way `kmp_curate` `mode: paths` returns
/// it: declared or proposed, with the proposed hop's `item_id` in the frozen
/// review.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PathHopDto {
    /// Where the hop starts, in walking order.
    pub from: PathEndDto,
    /// Where it arrives.
    pub to: PathEndDto,
    /// The declared relation, or the judge's choice for a proposed hop.
    #[serde(default)]
    pub rel: Option<String>,
    /// Whether a writer declared it.
    #[serde(default)]
    pub declared: bool,
    /// Whether the walk crosses it against its stored direction.
    #[serde(default)]
    pub reversed: bool,
    /// The judge's confidence; 1 for a declared hop.
    #[serde(default)]
    pub confidence: f64,
    /// A proposed hop's id in the frozen review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
}
