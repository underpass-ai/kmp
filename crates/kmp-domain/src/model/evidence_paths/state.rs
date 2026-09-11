use super::EvidencePathBindings;

/// Prefix depth advances along the shortest-context DAG; position advances
/// along declared steps. Their sum strictly increases on every predecessor.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct EvidencePathState {
    pub role: usize,
    pub position: u32,
    pub node: String,
    pub bindings: EvidencePathBindings,
    pub context_hops: u32,
}
