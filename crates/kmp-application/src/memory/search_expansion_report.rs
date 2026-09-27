use std::collections::BTreeMap;

use kmp_domain::SearchExpansions;

use super::{MemoryIngestOutcome, RefusedSearchExpansion, SearchExpansionProposal};

/// What became of a write's proposed search expansions.
///
/// An expansion is stored only after a judge accepted it against the stored
/// text (`SearchExpansions`). Every surface reads a proposal with the same
/// lint first; what passes it waits for a judge, and a surface without one
/// stores nothing and says why in `not_stored`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchExpansionReport {
    /// Kept expansions, by entry ref.
    pub stored: BTreeMap<String, Vec<String>>,
    /// What the lint or the judge refused, in the writer's order.
    pub refused: Vec<RefusedSearchExpansion>,
    /// Why what passed the lint was not stored, when that is so.
    pub not_stored: Option<String>,
    /// Which judge accepted what was kept, and at what bar.
    pub judged_by: Option<String>,
}

impl SearchExpansionReport {
    /// A preview commits nothing, so it judges and stores nothing.
    pub const PREVIEW_REASON: &'static str = "a preview judges and stores no expansion";

    /// The kernel's own write surface has no judge.
    pub const NO_JUDGE_REASON: &'static str = "this server cannot judge search expansions: \
         the kernel's write surface has no judge, so nothing is stored; kmp_write_memory \
         against an embedded store with Jev's opt-in keeps judged expansions";

    /// The report of a write whose surface has no judge. `None` when no
    /// entry proposed an expansion, or when nothing was committed yet (a
    /// write held for review).
    pub fn without_judge(
        proposals: &[SearchExpansionProposal],
        outcome: &MemoryIngestOutcome,
        dry_run: bool,
    ) -> Option<Self> {
        let proposals = proposals
            .iter()
            .filter(|proposal| !proposal.expansions.is_empty())
            .collect::<Vec<_>>();
        if proposals.is_empty() {
            return None;
        }
        if dry_run {
            return Some(Self {
                not_stored: Some(Self::PREVIEW_REASON.to_string()),
                ..Self::default()
            });
        }
        if !outcome.read_after_write_ready {
            return None;
        }
        let mut report = Self::default();
        let mut awaits_judge = false;
        for proposal in proposals {
            let (linted, refused) =
                SearchExpansions::lint_proposal(&proposal.text, &proposal.expansions);
            awaits_judge |= !linted.is_empty();
            report
                .refused
                .extend(
                    refused
                        .into_iter()
                        .map(|(expansion, why)| RefusedSearchExpansion {
                            entry_ref: proposal.entry_ref.clone(),
                            expansion,
                            why,
                        }),
                );
        }
        if awaits_judge {
            report.not_stored = Some(Self::NO_JUDGE_REASON.to_string());
        }
        Some(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryAcceptedCounts;

    const TEXT: &str = "The rollout slipped because the auditors had not signed off.";

    fn outcome(committed: bool) -> MemoryIngestOutcome {
        MemoryIngestOutcome {
            neighborhood: None,
            replayed: false,
            clocks: None,
            receipt_ref: None,
            about: "question:expansions".into(),
            memory_id: "memory".into(),
            accepted: MemoryAcceptedCounts {
                entries: 1,
                relations: 0,
                evidence: 0,
            },
            read_after_write_ready: committed,
            warnings: Vec::new(),
            created_dimensions: Vec::new(),
            resembling_labels: Vec::new(),
        }
    }

    fn proposal(expansions: &[&str]) -> SearchExpansionProposal {
        SearchExpansionProposal {
            entry_ref: "question:expansions:rollout".into(),
            text: TEXT.into(),
            expansions: expansions.iter().map(|text| text.to_string()).collect(),
        }
    }

    #[test]
    fn no_proposal_or_a_write_held_for_review_reports_nothing() {
        assert_eq!(
            SearchExpansionReport::without_judge(&[proposal(&[])], &outcome(true), false),
            None
        );
        assert_eq!(
            SearchExpansionReport::without_judge(
                &[proposal(&["Why was the launch postponed?"])],
                &outcome(false),
                false
            ),
            None
        );
    }

    #[test]
    fn a_preview_stores_nothing_and_says_so() {
        let report = SearchExpansionReport::without_judge(
            &[proposal(&["Why was the launch postponed?"])],
            &outcome(false),
            true,
        )
        .expect("report");
        assert!(report.stored.is_empty() && report.refused.is_empty());
        assert_eq!(
            report.not_stored.as_deref(),
            Some(SearchExpansionReport::PREVIEW_REASON)
        );
    }

    #[test]
    fn without_a_judge_the_lint_still_refuses_and_nothing_is_stored() {
        let report = SearchExpansionReport::without_judge(
            &[proposal(&[
                "Why was the launch postponed?",
                "the rollout slipped",
            ])],
            &outcome(true),
            false,
        )
        .expect("report");
        assert!(report.stored.is_empty());
        assert_eq!(report.judged_by, None);
        assert_eq!(
            report.refused,
            [RefusedSearchExpansion {
                entry_ref: "question:expansions:rollout".into(),
                expansion: "the rollout slipped".into(),
                why: "repeats the memory's words, which adds nothing to search".into(),
            }]
        );
        assert_eq!(
            report.not_stored.as_deref(),
            Some(SearchExpansionReport::NO_JUDGE_REASON)
        );
    }

    #[test]
    fn when_the_lint_refuses_everything_no_judge_is_missed() {
        let report = SearchExpansionReport::without_judge(
            &[proposal(&["the rollout slipped"])],
            &outcome(true),
            false,
        )
        .expect("report");
        assert_eq!(report.refused.len(), 1);
        assert_eq!(report.not_stored, None);
    }
}
