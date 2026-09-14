use super::{ClaimPolarity, EpistemicStatus};
use serde::{Deserialize, Serialize};

/// Every coordinate is supplied by a source-reading writer. A name is not an
/// entity identifier; temporal_scope must distinguish separate events/ownership.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimIdentity {
    pub referent: String,
    pub predicate: String,
    pub value: String,
    pub temporal_scope: String,
    pub polarity: ClaimPolarity,
    pub epistemic_status: EpistemicStatus,
    pub qualifiers: Vec<String>,
}
