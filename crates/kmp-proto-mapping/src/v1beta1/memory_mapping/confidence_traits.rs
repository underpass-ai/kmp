use super::concept_coverage::ConceptCoverage;
use super::confidence_branch::ConfidenceBranch;

/// What the kernel knows about an answer when it states its confidence:
/// only counts and flags, so every calibration rule over them is an integer
/// threshold or a yes/no, never a learned weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ConfidenceTraits {
    pub(super) branch: ConfidenceBranch,
    pub(super) coverage: ConceptCoverage,
    /// The question excluded an identifier (`excluding C7`).
    pub(super) negated_anchor: bool,
    /// Citations the answer core retained.
    pub(super) cited: usize,
    /// The question's contract reads it as asking for several things.
    pub(super) enumerative: bool,
}
