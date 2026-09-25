//! Drawn paths on the wire.

use serde::{Deserialize, Serialize};

use crate::view::application::dto::avoided_hop_dto::AvoidedHopDto;
use crate::view::application::dto::path_chain_dto::PathChainDto;

/// The paths facet of a view: what was asked (`from`, `to`, `max_hops`) and,
/// once the boundary ran the search, what `kmp_curate` `mode: paths` found.
/// In an intent as it arrives only the request is filled; the boundary adds
/// the answer before the aggregate is reached, so a retried intent digests
/// as the same request.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PathsDto {
    /// Where every chain starts.
    pub from: String,
    /// Where the chains must arrive, when a goal was named.
    #[serde(default)]
    pub to: Option<String>,
    /// The longest chain considered, when bounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_hops: Option<u8>,
    /// The frozen review the proposed hops belong to.
    #[serde(default)]
    pub review_token: Option<String>,
    /// The search's own one-line account.
    #[serde(default)]
    pub summary: String,
    /// The chains, fewest proposed hops first.
    #[serde(default)]
    pub paths: Vec<PathChainDto>,
    /// Declarations the walk left out.
    #[serde(default)]
    pub avoided: Vec<AvoidedHopDto>,
    /// What the search could not do.
    #[serde(default)]
    pub warnings: Vec<String>,
}
