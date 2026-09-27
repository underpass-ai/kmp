/// An about's collection statistics as the lexical index holds them: N, the
/// summed lengths of the two fields BM25 reads, and the language the rows
/// were read in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LexicalTotals {
    pub(crate) documents: u64,
    pub(crate) content_length: i64,
    pub(crate) direct_length: i64,
    pub(crate) language: Option<String>,
}
