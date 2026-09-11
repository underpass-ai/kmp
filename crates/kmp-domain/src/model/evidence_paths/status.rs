/// Structural task coverage, never semantic truth or corpus-wide absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidencePathStatus {
    Compatible,
    Ambiguous,
    ReviewRequired,
    MissingObligation,
    IncompatibleObligations,
    Partial,
}
