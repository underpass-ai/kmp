use std::fmt;

/// One reason a rendered question does not faithfully carry the user's words.
///
/// Reported together as a warning on the answer, written for the agent that
/// produced the rendering: each names what to change on the next ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionRenderingFault {
    /// The function words lean to a language other than the kernel's.
    NotEnglish { read: String },
    /// Not one informative word to search with.
    Empty,
    /// Identifiers the user's words carry and the rendering does not, folded
    /// and sorted.
    DropsIdentifiers(Vec<String>),
}

impl QuestionRenderingFault {
    /// All the faults of one rendering in one sentence, for a warning.
    pub fn describe(faults: &[Self]) -> String {
        faults
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    }
}

impl fmt::Display for QuestionRenderingFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnglish { read } => write!(formatter, "leans to {read}, not to English"),
            Self::Empty => write!(formatter, "carries no informative word"),
            Self::DropsIdentifiers(identifiers) => write!(
                formatter,
                "drops identifiers the user's words carry: {}",
                identifiers.join(", ")
            ),
        }
    }
}
