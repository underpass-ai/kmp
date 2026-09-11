use super::{EvidencePathCandidate, EvidencePathGroup, EvidencePathStatus};
use crate::{NodeRelationProjection, TraceSearchStop};

/// Native discovery and joint compatibility over one reader snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePathResult {
    pub proof: Option<crate::TraceProofResult>,
    pub from: String,
    pub context_discovery: bool,
    pub relations: Vec<NodeRelationProjection>,
    pub candidates: Vec<EvidencePathCandidate>,
    pub groups: Vec<EvidencePathGroup>,
    pub missing_roles: Vec<String>,
    pub status: EvidencePathStatus,
    pub stop: TraceSearchStop,
    pub discovered_nodes: u32,
    pub scanned_edges: u32,
    pub work_states: u32,
    pub shared_states: u32,
    pub incompatible_states: u32,
    pub adjacency_pages: u32,
    pub coordinate_pages: u32,
    pub resolved_as_of: Option<String>,
    pub temporal_selection_resolved: bool,
    pub clock_unknown_edges: Vec<u32>,
}

impl EvidencePathResult {
    pub fn known_complete(&self) -> bool {
        !self.context_discovery
            && self.status != EvidencePathStatus::Partial
            && self
                .groups
                .iter()
                .any(|g| g.bindings.missing.is_empty() && !g.clock_unknown)
    }
    pub fn viable_candidates(&self) -> std::collections::BTreeSet<u32> {
        self.groups
            .iter()
            .flat_map(|g| g.candidate_indexes.iter().copied())
            .collect()
    }
}
