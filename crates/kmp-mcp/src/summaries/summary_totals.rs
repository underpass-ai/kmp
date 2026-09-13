use super::audited_summary::AuditedSummary;
use super::summary_state::SummaryState;

/// How many memories stand where, over one selection or one about.
///
/// The doctor prints these and the audit returns them, from the same
/// reading: there is one counter here and no second implementation that
/// agrees with it by luck.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SummaryTotals {
    pub entries: usize,
    pub missing: usize,
    pub refused: usize,
    pub stands: usize,
    pub not_required: usize,
    /// Summaries the lint accepts that carry at least one weakness signal.
    pub weak: usize,
}

impl SummaryTotals {
    /// The totals of everything in `audited`.
    pub fn of<'a>(audited: impl IntoIterator<Item = &'a AuditedSummary>) -> Self {
        let mut totals = Self::default();
        for entry in audited {
            totals.entries += 1;
            match entry.state {
                SummaryState::Missing => totals.missing += 1,
                SummaryState::Refused => totals.refused += 1,
                SummaryState::Stands => totals.stands += 1,
                SummaryState::NotRequired => totals.not_required += 1,
            }
            if entry.state == SummaryState::Stands && !entry.weaknesses.is_empty() {
                totals.weak += 1;
            }
        }
        totals
    }

    /// How many memories owe a summary: the count the doctor reports and
    /// `summaries pending` lists.
    pub fn owed(&self) -> usize {
        self.missing + self.refused
    }
}

#[cfg(test)]
mod tests {
    use super::super::summary_weakness::SummaryWeakness;
    use super::*;

    fn audited(state: SummaryState, weaknesses: Vec<SummaryWeakness>) -> AuditedSummary {
        AuditedSummary {
            about: "project:a".to_string(),
            reference: "project:a:e1".to_string(),
            kind: "decision".to_string(),
            state,
            text: None,
            summary: None,
            summary_by: None,
            faults: Vec::new(),
            weaknesses,
        }
    }

    #[test]
    fn the_owed_count_is_what_is_missing_plus_what_the_lint_refuses() {
        let entries = [
            audited(SummaryState::Missing, Vec::new()),
            audited(SummaryState::Refused, Vec::new()),
            audited(SummaryState::Stands, vec![SummaryWeakness::Stale]),
            audited(SummaryState::Stands, Vec::new()),
            audited(SummaryState::NotRequired, Vec::new()),
        ];

        let totals = SummaryTotals::of(&entries);

        assert_eq!(totals.entries, 5);
        assert_eq!(totals.owed(), 2);
        assert_eq!(totals.weak, 1);
        assert_eq!(totals.stands, 2);
        assert_eq!(totals.not_required, 1);
    }
}
