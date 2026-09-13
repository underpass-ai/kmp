use std::fmt;

use kmp_domain::SearchSummary;

/// One reason a summary that clears the lint still carries little retrieval.
///
/// The lint is a floor, not a grade: English, two informative words, not a
/// copy of the text, every identifier kept. These are the shapes that stand
/// above that floor and still retrieve nothing, and each is derived from the
/// store's own text with no model and no network.
///
/// They are warnings and never refusals — the audit never rejects a memory —
/// and they live here rather than in the kernel's value objects because none
/// of them is a property of one summary. Each is read against the about
/// around it: its neighbours' summaries, its neighbours' words, or this
/// entry's own earlier revisions. `SearchSummaryFault` stays the vocabulary
/// of the lint, which is a pure reading of one text and one summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryWeakness {
    /// It clears the floor and little more, for a text many times its length.
    Thin {
        informative_terms: usize,
        text_terms: usize,
    },
    /// Other entries of the same about carry this summary, byte for byte.
    Repeated { other_entries: usize },
    /// Every informative word of it reaches most of the about's other
    /// entries, so none of them separates this memory from its neighbours.
    Undiscriminating { other_entries: usize },
    /// The text was rewritten after the summary was last written, so the
    /// summary describes a text the store no longer holds.
    Stale,
}

impl SummaryWeakness {
    /// The most informative words a rendering may carry and still be read as
    /// a tag rather than a rendering: twice the lint's floor.
    pub const THIN_CEILING: usize = SearchSummary::MINIMUM_INFORMATIVE_TERMS * 2;

    /// How many times the summary's informative words the text must carry
    /// before that ceiling means anything. Two words for a sentence is a
    /// summary; two words for a page is a label on a drawer.
    pub const THIN_TEXT_RATIO: usize = 8;

    /// The fewest entries an about must hold before "most of its entries"
    /// says anything. Below it, one neighbour is already most of them.
    pub const DISCRIMINATION_MINIMUM_ENTRIES: usize = 4;

    /// Every weakness of one summary in one sentence, for a warning.
    pub fn describe(weaknesses: &[Self]) -> String {
        weaknesses
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// The word this weakness is reported under, so a caller can branch on
    /// it without reading the sentence.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Thin { .. } => "thin",
            Self::Repeated { .. } => "repeated",
            Self::Undiscriminating { .. } => "undiscriminating",
            Self::Stale => "stale",
        }
    }
}

impl fmt::Display for SummaryWeakness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Thin {
                informative_terms,
                text_terms,
            } => write!(
                formatter,
                "carries {informative_terms} informative {} for a text of {text_terms}; name what \
                 this memory decides, and the identifiers a reader would ask with",
                if *informative_terms == 1 {
                    "word"
                } else {
                    "words"
                }
            ),
            Self::Repeated { other_entries } => write!(
                formatter,
                "is attached, word for word, to {other_entries} other {} of this about; render \
                 this memory's own text, not the batch it was written in",
                if *other_entries == 1 {
                    "memory"
                } else {
                    "memories"
                }
            ),
            Self::Undiscriminating { other_entries } => write!(
                formatter,
                "every one of its informative words already reaches most of this about's \
                 {other_entries} other memories; add the words only this one carries",
            ),
            Self::Stale => write!(
                formatter,
                "was written against an earlier version of this text, which has been rewritten \
                 since; render the text as it now stands"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wording is read by the writer that will replace the summary, so it
    /// names the change to make.
    #[test]
    fn every_weakness_says_what_to_change() {
        assert_eq!(
            SummaryWeakness::Thin {
                informative_terms: 2,
                text_terms: 40
            }
            .to_string(),
            "carries 2 informative words for a text of 40; name what this memory decides, and \
             the identifiers a reader would ask with"
        );
        assert_eq!(
            SummaryWeakness::Repeated { other_entries: 1 }.to_string(),
            "is attached, word for word, to 1 other memory of this about; render this memory's \
             own text, not the batch it was written in"
        );
        assert_eq!(
            SummaryWeakness::Undiscriminating { other_entries: 9 }.to_string(),
            "every one of its informative words already reaches most of this about's 9 other \
             memories; add the words only this one carries"
        );
        assert!(SummaryWeakness::Stale.to_string().contains("rewritten"));
        assert_eq!(SummaryWeakness::describe(&[]), "");
    }

    #[test]
    fn each_weakness_is_reportable_under_a_word_a_caller_can_branch_on() {
        assert_eq!(SummaryWeakness::Stale.name(), "stale");
        assert_eq!(
            SummaryWeakness::Repeated { other_entries: 3 }.name(),
            "repeated"
        );
    }

    /// The ceiling is anchored on the lint's floor rather than chosen: a
    /// rendering worth keeping carries more than twice the minimum.
    #[test]
    fn the_thresholds_are_anchored_on_the_lints_floor() {
        assert_eq!(SearchSummary::MINIMUM_INFORMATIVE_TERMS, 2);
        assert_eq!(SummaryWeakness::THIN_CEILING, 4);
    }
}
