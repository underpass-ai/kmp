use std::fmt;

/// One reason a proposed search expansion is not kept.
///
/// An expansion is a short way a later reader may ask for a memory: a
/// question it answers, a paraphrase, or a key in the other language. It is
/// a search surface and never an answer, so a proposal that says something
/// the memory does not is refused before any judge reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchExpansionFault {
    /// No informative word at all.
    Empty,
    /// Longer than an expansion may be.
    TooLong { chars: usize },
    /// It repeats the memory's own words, which adds nothing to search.
    RepeatsText,
    /// The same expansion was proposed twice for one memory.
    Duplicate,
    /// Identifiers the expansion names and the memory does not: a number, a
    /// version, a tag. An expansion that names them adds an answer.
    AddsIdentifiers(Vec<String>),
}

impl SearchExpansionFault {
    /// All the faults of one expansion in one sentence.
    pub fn describe(faults: &[Self]) -> String {
        faults
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    }
}

impl fmt::Display for SearchExpansionFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(formatter, "carries no informative word"),
            Self::TooLong { chars } => write!(
                formatter,
                "is {chars} characters long, at most {} are kept",
                super::SearchExpansions::MAX_CHARS
            ),
            Self::RepeatsText => write!(
                formatter,
                "repeats the memory's words, which adds nothing to search"
            ),
            Self::Duplicate => write!(formatter, "repeats another expansion of this memory"),
            Self::AddsIdentifiers(identifiers) => write!(
                formatter,
                "names identifiers the memory does not state: {}",
                identifiers.join(", ")
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fault_says_what_is_wrong() {
        assert_eq!(
            SearchExpansionFault::Empty.to_string(),
            "carries no informative word"
        );
        assert_eq!(
            SearchExpansionFault::TooLong { chars: 200 }.to_string(),
            "is 200 characters long, at most 120 are kept"
        );
        assert_eq!(
            SearchExpansionFault::RepeatsText.to_string(),
            "repeats the memory's words, which adds nothing to search"
        );
        assert_eq!(
            SearchExpansionFault::Duplicate.to_string(),
            "repeats another expansion of this memory"
        );
        assert_eq!(
            SearchExpansionFault::describe(&[
                SearchExpansionFault::Empty,
                SearchExpansionFault::AddsIdentifiers(vec!["#12".into()])
            ]),
            "carries no informative word; names identifiers the memory does not state: #12"
        );
    }
}
