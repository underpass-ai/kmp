/// Where one memory stands with respect to its English search summary.
///
/// Four states, and only two of them are a debt. The writer's rule decides
/// which: a memory whose text does not lean to English and carries no
/// summary cannot be reached from an English question; one whose summary the
/// lint refuses carries no retrieval whatever its language; an English
/// memory without a summary is already reachable as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryState {
    /// No summary, and the text does not lean to English. This memory owes
    /// one.
    Missing,
    /// No summary, and none is needed: an English question already lands on
    /// the text.
    NotRequired,
    /// A summary the lint refuses. It carries nothing, and the faults name
    /// what to change.
    Refused,
    /// A summary the lint accepts. Weakness signals may still be reported
    /// beside it; they are warnings, never a refusal.
    Stands,
}

impl SummaryState {
    /// Every state, in the order the audit reports them.
    pub const ALL: [Self; 4] = [
        Self::Missing,
        Self::Refused,
        Self::Stands,
        Self::NotRequired,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::NotRequired => "not_required",
            Self::Refused => "refused",
            Self::Stands => "stands",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.as_str() == value)
    }

    /// Whether this memory owes a summary. These are exactly the memories
    /// `summaries pending` lists and the doctor counts.
    pub fn owes_a_summary(self) -> bool {
        matches!(self, Self::Missing | Self::Refused)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_missing_or_refused_summary_is_a_debt() {
        assert!(SummaryState::Missing.owes_a_summary());
        assert!(SummaryState::Refused.owes_a_summary());
        assert!(!SummaryState::Stands.owes_a_summary());
        assert!(!SummaryState::NotRequired.owes_a_summary());
    }

    #[test]
    fn every_state_round_trips_through_its_word() {
        for state in SummaryState::ALL {
            assert_eq!(SummaryState::parse(state.as_str()), Some(state));
        }
        assert_eq!(SummaryState::parse("weak"), None);
    }
}
