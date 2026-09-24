/// Which pairs the proposer may read. `kmp_relate` proposes only across
/// abouts, because a proposal there is a comparison between two owners.
/// Curation also reads inside one about, where a missing relation is as real
/// as a missing equivalence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PairScope {
    AcrossAbouts,
    Any,
}
