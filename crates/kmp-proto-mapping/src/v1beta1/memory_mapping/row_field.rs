use super::counted_text::CountedText;
use super::lexical_row::LexicalRow;

/// A lexical index row's content or direct field under one reading, read in
/// place: the counts and the length `LexicalRow::content_counts` and
/// `LexicalRow::direct_counts` collect, without collecting them (P14: the
/// plan reads every candidate the postings reach).
#[derive(Clone, Copy)]
pub(super) struct RowField<'r> {
    row: &'r LexicalRow,
    aliased: bool,
    direct: bool,
}

impl<'r> RowField<'r> {
    pub(super) fn content(row: &'r LexicalRow, aliased: bool) -> Self {
        Self {
            row,
            aliased,
            direct: false,
        }
    }

    pub(super) fn direct(row: &'r LexicalRow, aliased: bool) -> Self {
        Self {
            row,
            aliased,
            direct: true,
        }
    }

    fn occurrences(&self, term: &super::lexical_term::LexicalTerm) -> i64 {
        if self.direct {
            term.direct(self.aliased)
        } else {
            term.content(self.aliased)
        }
    }

    /// The terms the field holds, ascending.
    pub(super) fn terms(&self) -> impl Iterator<Item = &'r String> + '_ {
        self.row
            .terms()
            .iter()
            .filter(|term| self.occurrences(term) > 0)
            .map(|term| &term.term)
    }
}

impl CountedText for RowField<'_> {
    fn count(&self, term: &str) -> u32 {
        self.row
            .term(term)
            .map(|held| self.occurrences(held))
            .filter(|occurrences| *occurrences > 0)
            .map_or(0, |occurrences| occurrences as u32)
    }

    fn length(&self) -> usize {
        let length = if self.direct {
            self.row.direct_length(self.aliased)
        } else {
            self.row.content_length(self.aliased)
        };
        length.max(0) as usize
    }
}
