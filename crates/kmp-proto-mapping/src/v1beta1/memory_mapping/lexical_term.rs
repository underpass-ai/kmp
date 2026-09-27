/// How often one term occurs in each part of a candidate's surface.
///
/// `text` counts the memory's own text. `summary` is what the content adds
/// to it and `extra` what the direct field adds to the content, so the two
/// fields BM25 reads are sums by construction: content = text + summary and
/// direct = text + summary + extra, exactly, whatever the tokenizer does at
/// the seams. Both residuals are almost always the plain count of the term in
/// the summary and in the source, refs and metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalTerm {
    pub term: String,
    pub text: i64,
    pub summary: i64,
    pub extra: i64,
}

impl LexicalTerm {
    /// Occurrences in the content field (text and summary).
    pub fn content(&self) -> i64 {
        self.text + self.summary
    }

    /// Occurrences in the direct field (content, source, refs, metadata).
    pub fn direct(&self) -> i64 {
        self.content() + self.extra
    }

    /// Whether either field BM25 reads carries the term.
    pub fn is_searchable(&self) -> bool {
        self.content() != 0 || self.direct() != 0
    }
}
