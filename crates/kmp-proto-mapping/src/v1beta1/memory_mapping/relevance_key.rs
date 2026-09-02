/// Ordered from the strongest evidence of relevance to the weakest.
///
/// Text still leads: nothing below `claim_matches` can lift a candidate over
/// one that answers the question in its own words. What the typed fields buy
/// is everything underneath — where the old key fell straight through to an
/// alphabetical tie-break on the ref.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RelevanceKey {
    pub(super) content_focus_matches: usize,
    /// BM25 of the question against the candidate's own text, in tenths of a
    /// point. It replaced a count of distinct shared concepts, which weighed
    /// a rare word exactly as much as a word every candidate uses.
    pub(super) content_score: i64,
    /// The same, over the text plus the identifiers and metadata a candidate
    /// is addressed by.
    pub(super) direct_score: i64,
    pub(super) claim_matches: usize,
    /// Relations of the kind the question asked for: a *why* question met by
    /// a causal edge, a replacement question met by `supersedes`.
    pub(super) intent_relation_matches: usize,
    pub(super) relation_why_matches: usize,
    pub(super) relation_matches: usize,
    /// The writer's own judgment of the matching relations, summed.
    pub(super) relation_signal: u32,
    pub(super) total_matches: usize,
    pub(super) recency_rank: u32,
}
