use super::EvidencePathBindings;

/// A complete declared role path, with original relation-table indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePathCandidate {
    pub role: usize,
    pub context_hops: u32,
    pub nodes: Vec<String>,
    pub edge_indexes: Vec<u32>,
    pub bindings: EvidencePathBindings,
    pub clock_unknown: bool,
}
