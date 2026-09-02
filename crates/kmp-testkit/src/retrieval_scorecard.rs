use std::collections::BTreeSet;

/// What one judged question produced, and how well.
///
/// Retrieval is an empirical field and this repository could not make an
/// empirical claim about its own retriever: every change to ranking was
/// justified by building a store by hand and showing that a mechanism fires.
/// That is evidence a thing works and no evidence about how much. These are
/// the numbers that turn one into the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalOutcome {
    /// The refs a reader judged as carrying the answer.
    pub judged: BTreeSet<String>,
    /// What the response returned, in the order it returned it.
    pub retrieved: Vec<String>,
    /// What the answer actually cited, which is a stricter question than
    /// whether the evidence came back at all.
    pub cited: BTreeSet<String>,
    pub unknown: bool,
    pub used_bytes: u64,
    pub elapsed_millis: u64,
}

impl RetrievalOutcome {
    /// The share of judged refs that appear in the first `k` returned.
    pub fn recall_at(&self, k: usize) -> f64 {
        if self.judged.is_empty() {
            return 0.0;
        }
        let seen = self
            .retrieved
            .iter()
            .take(k)
            .filter(|item| self.judged.contains(*item))
            .collect::<BTreeSet<_>>()
            .len();
        seen as f64 / self.judged.len() as f64
    }

    /// One over the rank of the first judged ref, or zero when none came back.
    pub fn reciprocal_rank(&self) -> f64 {
        self.retrieved
            .iter()
            .position(|item| self.judged.contains(item))
            .map_or(0.0, |index| 1.0 / (index + 1) as f64)
    }

    /// Normalized discounted cumulative gain over binary relevance.
    ///
    /// Recall says whether the answer came back; nDCG says whether it came
    /// back near the top, which is what a reader with a token budget actually
    /// receives.
    pub fn ndcg_at(&self, k: usize) -> f64 {
        if self.judged.is_empty() {
            return 0.0;
        }
        let gain = |index: usize| 1.0 / ((index + 2) as f64).log2();
        let earned = self
            .retrieved
            .iter()
            .take(k)
            .enumerate()
            .filter(|(_, item)| self.judged.contains(*item))
            .map(|(index, _)| gain(index))
            .sum::<f64>();
        let ideal = (0..self.judged.len().min(k)).map(gain).sum::<f64>();
        if earned <= 0.0 || ideal <= 0.0 {
            return 0.0;
        }
        earned / ideal
    }

    /// Whether the answer cited what it was supposed to, rather than merely
    /// retrieving it into the proof.
    pub fn answer_cites_judged(&self) -> bool {
        self.cited.iter().any(|item| self.judged.contains(item))
    }

    /// The question this repository could not answer: how often memory says it
    /// does not know something it demonstrably holds.
    ///
    /// A case is only counted when a reader judged a ref for it, so the answer
    /// is present by construction and UNKNOWN can only be a retrieval failure.
    pub fn is_false_unknown(&self) -> bool {
        self.unknown && !self.judged.is_empty()
    }
}

/// The aggregate over a judged collection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetrievalScorecard {
    pub cases: usize,
    pub recall_at_1: f64,
    pub recall_at_5: f64,
    pub recall_at_10: f64,
    pub mean_reciprocal_rank: f64,
    pub ndcg_at_10: f64,
    pub answer_core_precision: f64,
    pub false_unknown_rate: f64,
    pub mean_used_bytes: f64,
    pub mean_elapsed_millis: f64,
}

impl RetrievalScorecard {
    pub fn score(outcomes: &[RetrievalOutcome]) -> Self {
        let cases = outcomes.len();
        let mean = |value: &dyn Fn(&RetrievalOutcome) -> f64| {
            if cases == 0 {
                0.0
            } else {
                outcomes.iter().map(value).sum::<f64>() / cases as f64
            }
        };
        Self {
            cases,
            recall_at_1: mean(&|outcome| outcome.recall_at(1)),
            recall_at_5: mean(&|outcome| outcome.recall_at(5)),
            recall_at_10: mean(&|outcome| outcome.recall_at(10)),
            mean_reciprocal_rank: mean(&RetrievalOutcome::reciprocal_rank),
            ndcg_at_10: mean(&|outcome| outcome.ndcg_at(10)),
            answer_core_precision: mean(&|outcome| f64::from(outcome.answer_cites_judged())),
            false_unknown_rate: mean(&|outcome| f64::from(outcome.is_false_unknown())),
            mean_used_bytes: mean(&|outcome| outcome.used_bytes as f64),
            mean_elapsed_millis: mean(&|outcome| outcome.elapsed_millis as f64),
        }
    }

    /// The quality columns, in the order the recorded baseline stores them.
    ///
    /// Cost is deliberately not here: a floor that rose because a response got
    /// bigger would be a gate rewarding waste.
    pub fn quality_columns(&self) -> [(&'static str, f64); 6] {
        [
            ("recall_at_1", self.recall_at_1),
            ("recall_at_5", self.recall_at_5),
            ("recall_at_10", self.recall_at_10),
            ("mean_reciprocal_rank", self.mean_reciprocal_rank),
            ("ndcg_at_10", self.ndcg_at_10),
            ("answer_core_precision", self.answer_core_precision),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(
        judged: &[&str],
        retrieved: &[&str],
        cited: &[&str],
        unknown: bool,
    ) -> RetrievalOutcome {
        RetrievalOutcome {
            judged: judged.iter().map(|item| (*item).to_string()).collect(),
            retrieved: retrieved.iter().map(|item| (*item).to_string()).collect(),
            cited: cited.iter().map(|item| (*item).to_string()).collect(),
            unknown,
            used_bytes: 0,
            elapsed_millis: 0,
        }
    }

    #[test]
    fn recall_counts_only_what_arrived_inside_the_cutoff() {
        let found = outcome(&["a"], &["x", "y", "a"], &[], false);

        assert_eq!(found.recall_at(1), 0.0);
        assert_eq!(found.recall_at(5), 1.0);
    }

    #[test]
    fn recall_is_a_share_when_more_than_one_ref_was_judged() {
        let half = outcome(&["a", "b"], &["a", "x"], &[], false);

        assert_eq!(half.recall_at(5), 0.5);
    }

    #[test]
    fn rank_is_rewarded_and_absence_scores_nothing() {
        let first = outcome(&["a"], &["a", "x"], &[], false);
        let third = outcome(&["a"], &["x", "y", "a"], &[], false);
        let missing = outcome(&["a"], &["x", "y"], &[], false);

        assert_eq!(first.reciprocal_rank(), 1.0);
        assert!((third.reciprocal_rank() - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(missing.reciprocal_rank(), 0.0);
        assert!(first.ndcg_at(10) > third.ndcg_at(10));
        assert_eq!(missing.ndcg_at(10), 0.0);
        assert_eq!(first.ndcg_at(10), 1.0);
    }

    /// Retrieving the answer into the proof and citing it are different
    /// claims, and the graph traversal makes the difference deliberate.
    #[test]
    fn citing_is_stricter_than_retrieving() {
        let reached = outcome(&["a"], &["a"], &[], false);
        let cited = outcome(&["a"], &["a"], &["a"], false);

        assert_eq!(reached.recall_at(5), 1.0);
        assert!(!reached.answer_cites_judged());
        assert!(cited.answer_cites_judged());
    }

    #[test]
    fn unknown_is_false_only_when_the_answer_was_judged_to_exist() {
        let withheld = outcome(&["a"], &[], &[], true);
        let honestly_absent = outcome(&[], &[], &[], true);

        assert!(withheld.is_false_unknown());
        assert!(!honestly_absent.is_false_unknown());
    }

    #[test]
    fn a_scorecard_averages_over_the_collection() {
        let card = RetrievalScorecard::score(&[
            outcome(&["a"], &["a"], &["a"], false),
            outcome(&["b"], &["x"], &[], true),
        ]);

        assert_eq!(card.cases, 2);
        assert_eq!(card.recall_at_5, 0.5);
        assert_eq!(card.answer_core_precision, 0.5);
        assert_eq!(card.false_unknown_rate, 0.5);
        assert_eq!(card.quality_columns().len(), 6);
    }

    #[test]
    fn an_empty_collection_scores_zero_rather_than_dividing_by_it() {
        let card = RetrievalScorecard::score(&[]);

        assert_eq!(card.cases, 0);
        assert_eq!(card.recall_at_5, 0.0);
        assert_eq!(card.ndcg_at_10, 0.0);
    }
}
