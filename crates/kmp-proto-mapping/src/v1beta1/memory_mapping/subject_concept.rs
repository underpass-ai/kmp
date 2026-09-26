/// One concept a question asks of its principal anchor.
///
/// `literal` marks a word the stemmer shortened by more than an inflection
/// onto a stem the concept table happens to list as a word of its own:
/// `correctness` stems to `correct`, and the table reads `correct` as the
/// verb it groups with `correction` and `fix`. The table lists surface words,
/// so a stem reaches it only through an inflection (`fixes`, `corrects`);
/// a derived word is compared with its stem as written and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SubjectConcept {
    /// The search key the ranker compares it in.
    pub(super) key: String,
    /// The reader's word for it, as the question wrote it.
    pub(super) written: String,
    /// Compared as the stem itself, never through the concept table.
    pub(super) literal: bool,
}
