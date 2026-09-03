use std::fmt;

/// One reason a search summary will not carry retrieval.
///
/// Each names what the writer has to change. They are reported together so a
/// summary is fixed in one pass, and they are written to be read by the
/// writer that produced the summary, which is usually a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchSummaryFault {
    /// The function words lean to another language.
    NotEnglish { read: String },
    /// Fewer informative words than a rendering needs.
    Thin { informative_terms: usize },
    /// The same informative words as the text, in the same order.
    RepeatsText,
    /// Identifiers the text carries and the summary does not, folded and
    /// sorted.
    DropsIdentifiers(Vec<String>),
}

impl SearchSummaryFault {
    /// All the faults of one summary in one sentence, for a warning.
    pub fn describe(faults: &[Self]) -> String {
        faults
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    }
}

impl fmt::Display for SearchSummaryFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnglish { read } => {
                write!(formatter, "leans to {read}, not to English")
            }
            Self::Thin { informative_terms } => write!(
                formatter,
                "carries {informative_terms} informative {}, at least {} are needed",
                if *informative_terms == 1 {
                    "word"
                } else {
                    "words"
                },
                super::SearchSummary::MINIMUM_INFORMATIVE_TERMS
            ),
            Self::RepeatsText => write!(
                formatter,
                "repeats the text word for word, which adds nothing to search"
            ),
            Self::DropsIdentifiers(identifiers) => write!(
                formatter,
                "drops identifiers the text carries: {}",
                identifiers.join(", ")
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wording is read by the writer that produced the summary, so it
    /// names the change to make.
    #[test]
    fn every_fault_says_what_to_change() {
        assert_eq!(
            SearchSummaryFault::NotEnglish {
                read: "spanish".to_string()
            }
            .to_string(),
            "leans to spanish, not to English"
        );
        assert_eq!(
            SearchSummaryFault::Thin {
                informative_terms: 0
            }
            .to_string(),
            "carries 0 informative words, at least 2 are needed"
        );
        assert_eq!(
            SearchSummaryFault::RepeatsText.to_string(),
            "repeats the text word for word, which adds nothing to search"
        );
        assert_eq!(
            SearchSummaryFault::DropsIdentifiers(vec!["#469".to_string(), "v0.7.0".to_string()])
                .to_string(),
            "drops identifiers the text carries: #469, v0.7.0"
        );
        assert_eq!(SearchSummaryFault::describe(&[]), "");
    }
}
