/// How a memory states a concept the anchored gate asks of it.
///
/// The gate decides on its own words first. A verdict that stands only
/// because the lexical bridge translated a concept the memory states in
/// another language is still a verdict, and the response says so by never
/// reading it as surer than `medium` ([`super::gate_verdict::GateVerdict`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StatedBy {
    /// In the reader's words: the stem, or the concept the table of
    /// concepts groups it with.
    OwnWords,
    /// Through the lexical bridge, onto a memory written in another language
    /// than the question ([`super::cross_language::CrossLanguage`]).
    Translation,
}
