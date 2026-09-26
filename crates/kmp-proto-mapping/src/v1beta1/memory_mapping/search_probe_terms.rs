use std::collections::BTreeSet;

/// What the kernel's tokenizer makes of one text, read through a
/// [`SearchProbe`](super::search_probe::SearchProbe).
///
/// Every field is a sorted set because the ranker compares sets: order and
/// repetition never reach a score. The fields are layered, each one a later
/// step of the same pipeline, so a benchmark can tell which step lost a word.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchProbeTerms {
    /// The folded tokens that survive the stop-word and length filters,
    /// before any concept table or stemmer touches them.
    pub informative_terms: BTreeSet<String>,
    /// Each informative term mapped through the hand-kept concept table
    /// alone; a word the table does not know stays as it is.
    pub concept_keys: BTreeSet<String>,
    /// The form the ranker compares: the concept key when the table knows
    /// the word, otherwise its stem under the probe's morphology. Equal to
    /// the text terms `AnswerCandidateTerms` builds for the same text.
    pub search_keys: BTreeSet<String>,
    /// The identifiers the text carries, folded, as
    /// `kmp_domain::language::identifiers` reads them.
    pub identifiers: BTreeSet<String>,
    /// The identifiers the ranker also reads as whole search terms, as
    /// `kmp_domain::language::compound_identifiers` yields them: `c6.24`,
    /// `0.7.0`, `c6.8` and `c6.9` from `C6.8+C6.9`. Each is in
    /// `search_keys` as written, unstemmed.
    pub compound_identifiers: BTreeSet<String>,
}
