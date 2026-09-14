use crate::TraceCondenseCandidate;

/// Whole-selection recommendations. Counts are disjoint and cover only objects
/// with a stored body and a card presentation for the requested language.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceCondenseCandidates {
    pub items: Vec<TraceCondenseCandidate>,
    pub omitted_count: u32,
    pub below_floor: u32,
    pub valid: u32,
    pub after_cut: u32,
}
