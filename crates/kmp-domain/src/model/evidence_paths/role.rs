use super::EvidencePathBinding;
use crate::TraceRelationStep;

/// An explicit finite task role. Steps reuse native relation types/directions.
/// This internal policy is not natural-language understanding or a query DSL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePathRole {
    pub name: String,
    pub steps: Vec<TraceRelationStep>,
    pub bindings: Vec<EvidencePathBinding>,
}
