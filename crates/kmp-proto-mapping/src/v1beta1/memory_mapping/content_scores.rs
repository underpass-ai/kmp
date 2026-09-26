use std::collections::BTreeMap;

/// The BM25 content score, in tenths, each candidate a question reached in
/// its own words was ranked with, by evidence id.
///
/// The ranker orders by these and forgets them; the doubt band reads the
/// distance between the first two citations (DESIGN L4 4c): a core whose
/// leader barely beats its runner-up is a ranking a judge may be asked about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ContentScores {
    scores: BTreeMap<String, i64>,
    /// The first two ids of the ranking, in rank order.
    leaders: Vec<String>,
}

impl ContentScores {
    /// Scores in rank order; an id read twice keeps its first score.
    pub(super) fn read<'i>(scored: impl IntoIterator<Item = (&'i str, i64)>) -> Self {
        let mut read = Self::default();
        for (id, score) in scored {
            if read.scores.contains_key(id) {
                continue;
            }
            read.scores.insert(id.to_string(), score);
            if read.leaders.len() < 2 {
                read.leaders.push(id.to_string());
            }
        }
        read
    }

    /// How far the first citation leads the second, in tenths. A lone
    /// citation leads nothing but zero; an id the ranking never scored (a
    /// restatement, a judged citation) counts as zero.
    pub(super) fn margin<'i>(&self, core: impl IntoIterator<Item = &'i str>) -> i64 {
        let mut scores = core
            .into_iter()
            .take(2)
            .map(|id| self.scores.get(id).copied().unwrap_or(0));
        let first = scores.next().unwrap_or(0);
        first - scores.next().unwrap_or(0)
    }

    /// How far the first candidate of the ranking leads the second, in
    /// tenths; `None` when the ranking scored nothing.
    pub(super) fn lead(&self) -> Option<i64> {
        (!self.leaders.is_empty()).then(|| self.margin(self.leaders.iter().map(String::as_str)))
    }
}

#[cfg(test)]
mod tests {
    use super::ContentScores;

    #[test]
    fn the_margin_is_the_lead_of_the_first_citation_over_the_second() {
        let scores = ContentScores::read([("a", 90), ("b", 70), ("a", 5), ("c", 10)]);
        assert_eq!(scores.margin(["a", "b"]), 20);
        assert_eq!(scores.margin(["b", "a", "c"]), -20);
        assert_eq!(scores.margin(["c"]), 10, "a lone citation leads zero");
        assert_eq!(
            scores.margin(["x", "a"]),
            -90,
            "an unscored id counts as zero"
        );
        assert_eq!(scores.margin(std::iter::empty()), 0);
    }
}
