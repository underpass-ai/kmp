//! A path step's end on the wire.

use serde::{Deserialize, Serialize};

/// A fact a path passes through, spelled the way `kmp_curate` returns it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PathEndDto {
    /// The fact's ref.
    #[serde(rename = "ref")]
    pub reference: String,
    /// The about that owns it.
    #[serde(default)]
    pub about: String,
    /// Its text as the path search excerpted it.
    #[serde(default)]
    pub excerpt: String,
}
