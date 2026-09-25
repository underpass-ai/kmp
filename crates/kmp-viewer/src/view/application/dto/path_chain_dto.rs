//! A whole chain on the wire.

use serde::{Deserialize, Serialize};

use crate::view::application::dto::path_hop_dto::PathHopDto;

/// One chain as `kmp_curate` returns it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PathChainDto {
    /// The hops, first to last.
    pub hops: Vec<PathHopDto>,
    /// How many hops the judge proposes.
    #[serde(default)]
    pub proposed: usize,
    /// The product of the hops' confidences.
    #[serde(default)]
    pub confidence: f64,
}
