/// How much of a question the best retained memory covers, in concepts.
///
/// `matched` is the most question concepts any single retained memory is
/// credited with (identifier parts only when it names the identifier);
/// `asked` is how many concepts the question has. Both are counts, so a rule
/// over them is an integer threshold, never a fitted weight.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ConceptCoverage {
    pub(super) matched: usize,
    pub(super) asked: usize,
}

impl ConceptCoverage {
    /// The share of the question covered; zero when it asks nothing.
    pub(super) fn share(&self) -> f64 {
        if self.asked == 0 {
            0.0
        } else {
            self.matched as f64 / self.asked as f64
        }
    }

    /// The question concepts the best memory leaves out.
    pub(super) fn missed(&self) -> usize {
        self.asked.saturating_sub(self.matched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_and_missed_are_read_from_the_counts() {
        let coverage = ConceptCoverage {
            matched: 3,
            asked: 5,
        };
        assert!((coverage.share() - 0.6).abs() < f64::EPSILON);
        assert_eq!(coverage.missed(), 2);
        assert_eq!(ConceptCoverage::default().share(), 0.0);
        assert_eq!(
            ConceptCoverage {
                matched: 4,
                asked: 2
            }
            .missed(),
            0
        );
    }
}
