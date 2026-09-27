use std::collections::BTreeMap;

use super::lexical_margin::LexicalMargin;

/// The BM25 content score, in tenths, each candidate a question reached in
/// its own words was ranked with, by evidence id, and which two led the
/// ranking.
///
/// The ranker orders by these and forgets them; Ask reads its one margin
/// from them ([`LexicalMargin`], DESIGN L4 4c): the lead of the ranking's
/// first candidate for the re-ranking gate, the lead of an answer's first
/// citation for the doubt band.
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

    /// How far the first of `cited` leads the second, in tenths: the only
    /// place a margin is computed. A lone id leads by its whole score, an id
    /// the ranking never scored (a restatement, a judged citation) counts as
    /// zero, and nothing cited leads by nothing (`None`). The confidence is
    /// not known here: it reads as not `High`.
    pub(super) fn margin<'i>(&self, cited: impl IntoIterator<Item = &'i str>) -> LexicalMargin {
        let mut scores = cited
            .into_iter()
            .take(2)
            .map(|id| self.scores.get(id).copied().unwrap_or(0));
        let tenths = scores
            .next()
            .map(|first| first - scores.next().unwrap_or(0));
        LexicalMargin {
            tenths,
            high_confidence: false,
        }
    }

    /// The margin of the ranking itself: its first eligible candidate over
    /// the second.
    pub(super) fn lead(&self) -> LexicalMargin {
        self.margin(self.leaders.iter().map(String::as_str))
    }
}

#[cfg(test)]
mod tests {
    use super::ContentScores;

    #[test]
    fn the_margin_is_the_lead_of_the_first_citation_over_the_second() {
        let scores = ContentScores::read([("a", 90), ("b", 70), ("a", 5), ("c", 10)]);
        let tenths = |cited: &[&str]| scores.margin(cited.iter().copied()).tenths;
        assert_eq!(tenths(&["a", "b"]), Some(20));
        assert_eq!(tenths(&["b", "a", "c"]), Some(-20));
        assert_eq!(
            tenths(&["c"]),
            Some(10),
            "a lone citation leads by its score"
        );
        assert_eq!(
            tenths(&["x", "a"]),
            Some(-90),
            "an unscored id counts as zero"
        );
        assert_eq!(tenths(&[]), None, "nothing cited leads by nothing");
        assert!(!scores.margin(["a"]).high_confidence);
    }

    #[test]
    fn the_lead_reads_the_first_two_candidates_of_the_ranking() {
        let scores = ContentScores::read([("a", 90), ("b", 70), ("a", 5), ("c", 10)]);
        assert_eq!(scores.lead().tenths, Some(20));
        assert_eq!(ContentScores::read([("a", 7)]).lead().tenths, Some(7));
        assert_eq!(ContentScores::default().lead().tenths, None);
    }
}
