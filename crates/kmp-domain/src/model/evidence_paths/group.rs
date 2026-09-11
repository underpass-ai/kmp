use super::EvidencePathBindings;

/// One candidate per required role. Missing witnesses keep the group for review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePathGroup {
    pub candidate_indexes: Vec<u32>,
    pub bindings: EvidencePathBindings,
    pub clock_unknown: bool,
}
