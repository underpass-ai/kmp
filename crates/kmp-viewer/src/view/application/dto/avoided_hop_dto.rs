//! An avoided declaration on the wire.

use serde::{Deserialize, Serialize};

use crate::view::application::dto::path_end_dto::PathEndDto;

/// A declared relation the path walk left out because the judge found its
/// why and evidence do not hold, spelled the way `kmp_curate` returns it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AvoidedHopDto {
    /// The declaration's source.
    pub from: PathEndDto,
    /// Its target.
    pub to: PathEndDto,
    /// The declared relation.
    #[serde(default)]
    pub rel: Option<String>,
    /// The judge's probability that its why and evidence hold.
    #[serde(default)]
    pub support: f64,
}
