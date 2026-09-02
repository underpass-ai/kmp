/// Whether a candidate still stands.
///
/// KMP models two ways of ceasing to be current advice and its own guide
/// insists they are different: something replaced this one, or it simply ran
/// out with nothing in its place. A reader asking about now wants neither; a
/// reader asking about history wants exactly them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CandidateTemporalState {
    CurrentOrUnspecified,
    Superseded,
    /// Applicability ended without a replacement. `valid_until` is the only
    /// lifecycle KMP models that names no successor, so it cannot be found
    /// by following `supersedes`.
    Expired,
}
