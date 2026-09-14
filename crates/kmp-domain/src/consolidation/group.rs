use super::{ClaimIdentity, ConsolidationAssertion};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidatedClaim {
    pub identity: ClaimIdentity,
    pub supports: Vec<ConsolidationAssertion>,
}
