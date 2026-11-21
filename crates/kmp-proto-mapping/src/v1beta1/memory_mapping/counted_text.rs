/// One field of a candidate as BM25 reads it: how often a term occurs in
/// it, and how long it is. Counts collected into [`super::term_counts::TermCounts`]
/// and counts read in place from a lexical index row
/// ([`super::row_field::RowField`]) score alike, to the bit.
pub(super) trait CountedText {
    fn count(&self, term: &str) -> u32;
    fn length(&self) -> usize;
}

impl CountedText for super::term_counts::TermCounts {
    fn count(&self, term: &str) -> u32 {
        super::term_counts::TermCounts::count(self, term)
    }

    fn length(&self) -> usize {
        super::term_counts::TermCounts::length(self)
    }
}
