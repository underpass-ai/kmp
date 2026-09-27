/// How often one term occurs in each part of a candidate's surface, under
/// both readings an ask can take: plain, and with the alias terms a text
/// spells (`corte 10` as `c10`), which the anchored gate reads.
///
/// `text` counts the memory's own text. `summary` is what the plain content
/// adds to it and `extra` what the plain direct field adds to the content.
/// `alias_content` is what the aliases add to the content and `alias_extra`
/// what they add to the rest of the direct field. Every part is a residual,
/// so the fields BM25 reads are sums by construction, exactly, whatever the
/// tokenizer does at the seams:
///
/// - plain: content = text + summary, direct = content + extra;
/// - aliased: content = text + summary + alias_content,
///   direct = content + extra + alias_extra.
///
/// `expansion` counts the memory's judged search expansions (P15, field X),
/// a surface of its own that is part of neither field and reads the same
/// under both readings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalTerm {
    pub term: String,
    pub text: i64,
    pub summary: i64,
    pub extra: i64,
    pub alias_content: i64,
    pub alias_extra: i64,
    pub expansion: i64,
}

impl LexicalTerm {
    /// Occurrences in the content field (text and summary) under a reading.
    pub fn content(&self, aliased: bool) -> i64 {
        self.text + self.summary + if aliased { self.alias_content } else { 0 }
    }

    /// Occurrences in the direct field (content, source, refs, metadata).
    pub fn direct(&self, aliased: bool) -> i64 {
        self.content(aliased) + self.extra + if aliased { self.alias_extra } else { 0 }
    }

    /// Whether either field BM25 reads carries the term under a reading.
    pub fn is_searchable(&self, aliased: bool) -> bool {
        self.content(aliased) != 0 || self.direct(aliased) != 0
    }

    /// Whether the judged expansions carry the term.
    pub fn is_expanded(&self) -> bool {
        self.expansion != 0
    }

    /// Whether some reading or the expansions carry the term: what a posting
    /// is kept for, so the postings reach every candidate an ask can score
    /// or rescue through its expansions.
    pub fn is_held(&self) -> bool {
        self.is_searchable(false) || self.is_searchable(true) || self.is_expanded()
    }
}
