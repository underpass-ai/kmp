use super::EvidencePathBinding;
use crate::TraceRelationStep;

/// An explicit finite task role. Steps reuse native relation types/directions.
/// This internal policy is not natural-language understanding or a query DSL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePathRole {
    pub name: String,
    /// Discover minimum-hop context prefixes before the declared steps. These
    /// are navigation leads, not implicit logical composition of relations.
    pub context: bool,
    pub steps: Vec<TraceRelationStep>,
    pub bindings: Vec<EvidencePathBinding>,
}
