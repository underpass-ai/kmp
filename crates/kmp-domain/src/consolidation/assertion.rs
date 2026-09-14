use super::ClaimIdentity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsolidationAssertion {
    pub source_ref: String,
    /// Literal, nonempty substring of the stored source body.
    pub quote: String,
    pub claim: ClaimIdentity,
    /// Writer's rationale connecting this interpretation to the quote.
    pub why: String,
}
