use std::collections::BTreeSet;

/// Caller-declared OR of AND sets of known destinations, not a truth assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceProofRequirement {
    pub alternatives: Vec<BTreeSet<String>>,
    pub weight: u32,
}
