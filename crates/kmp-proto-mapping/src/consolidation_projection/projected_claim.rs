use kmp_domain::consolidation::ClaimIdentity;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProjectedClaim {
    pub identity: ClaimIdentity,
    pub source_refs: Vec<String>,
    pub source_states: std::collections::BTreeMap<String, String>,
    /// Literal lifecycle links; repetition does not imply independence.
    pub lifecycle_relations: Vec<(Vec<String>, std::collections::BTreeMap<String, String>)>,
}
