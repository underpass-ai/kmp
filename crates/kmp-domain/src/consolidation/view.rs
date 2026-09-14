use super::{ConsolidatedClaim, ConsolidationSource};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidatedView {
    pub about: String,
    pub view: String,
    pub revision: u64,
    pub authored_at: String,
    pub author: String,
    pub claims: Vec<ConsolidatedClaim>,
    /// Immutable source copies make prior versions auditable after replacement.
    pub sources: Vec<ConsolidationSource>,
}
