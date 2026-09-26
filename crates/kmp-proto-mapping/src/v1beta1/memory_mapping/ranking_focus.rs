use std::collections::BTreeSet;

/// What a question asks a candidate to be about, beyond its words: the focus
/// a strict policy requires, and the entry kinds the anchored gate prefers on
/// a tie.
#[derive(Clone, Copy, Default)]
pub(super) struct RankingFocus<'a> {
    /// The focus concepts and how many of them a candidate must answer.
    pub(super) strict: Option<&'a (BTreeSet<String>, usize)>,
    /// The entry kinds that state a facet the question enumerates.
    pub(super) facet_kinds: Option<&'a BTreeSet<String>>,
}
