use kmp_domain::SearchSummaryFault;

use super::summary_state::SummaryState;
use super::summary_weakness::SummaryWeakness;

/// One memory, and where it stands with respect to its English search
/// summary.
///
/// `faults` is the lint's own vocabulary, byte for byte the sentences
/// `kmp_ingest` and `kmp_write_memory` already put in front of a writer, so
/// a caller repairing a summary reads the same words wherever it met them.
/// `weaknesses` is separate on purpose: a fault means the summary carries
/// nothing, a weakness means it carries little. Neither rejects the memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditedSummary {
    pub about: String,
    pub reference: String,
    pub kind: String,
    pub state: SummaryState,
    /// The stored text, present when this memory needs a rendering written,
    /// so the writer does not have to fetch it again. Absent for a memory
    /// with nothing to do: its text is not what the answer is about.
    pub text: Option<String>,
    /// The summary as stored, when there is one.
    pub summary: Option<String>,
    /// Who wrote that summary, when the store knows.
    pub summary_by: Option<String>,
    pub faults: Vec<SearchSummaryFault>,
    pub weaknesses: Vec<SummaryWeakness>,
}

impl AuditedSummary {
    /// Whether this memory owes a summary that does not exist or does not
    /// carry.
    pub fn owes_a_summary(&self) -> bool {
        self.state.owes_a_summary()
    }

    /// Whether a writer has something to do here: a debt, or a summary that
    /// stands but retrieves little.
    pub fn needs_a_writer(&self) -> bool {
        self.owes_a_summary() || !self.weaknesses.is_empty()
    }

    /// The lint's refusal as the sentences it speaks, for a surface that
    /// renders them. Every surface reads the same words, because there is
    /// one place that turns the faults into words.
    pub fn fault_sentences(&self) -> Vec<String> {
        self.faults
            .iter()
            .map(SearchSummaryFault::to_string)
            .collect()
    }
}

#[cfg(test)]
mod tests {
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
    fn a_summary_that_stands_weakly_needs_a_writer_without_owing_one() {
        let weak = audited(SummaryState::Stands, vec![SummaryWeakness::Stale]);

        assert!(!weak.owes_a_summary());
        assert!(weak.needs_a_writer());
    }

    #[test]
    fn a_summary_that_stands_cleanly_needs_nobody() {
        assert!(!audited(SummaryState::Stands, Vec::new()).needs_a_writer());
        assert!(!audited(SummaryState::NotRequired, Vec::new()).needs_a_writer());
    }
}
