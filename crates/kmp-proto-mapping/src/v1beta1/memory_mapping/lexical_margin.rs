/// How decisively the first of two lexical citations leads the second: the
/// difference of their BM25 content scores in whole tenths of a point
/// (DESIGN L4 4c, after AgentIR 2605.25092), and whether the answer standing
/// on them is of `High` confidence.
///
/// It is the one margin Ask reads, always from `ContentScores::margin`
/// (`content_scores.rs`), and each reader keeps its own rule:
///
/// - the re-ranking gate ([`Self::is_decisive`]) sends no judgement when the
///   ranking's first eligible candidate leads by at least the store's `τ`
///   and the confidence is `High`: a remote judge adds nothing to an answer
///   the text already settles;
/// - the doubt band (`is_narrow`) asks its judge about an answer
///   whose first citation leads the second by less than `τ_m`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexicalMargin {
    /// `None` when nothing was cited or eligible. A lone one leads by its
    /// whole score; an id the ranking never scored counts as zero.
    pub tenths: Option<i64>,
    pub high_confidence: bool,
}

impl LexicalMargin {
    /// The margin with the confidence of the answer it stands on.
    pub(super) fn with_high_confidence(self, high_confidence: bool) -> Self {
        Self {
            high_confidence,
            ..self
        }
    }

    /// Whether the lead settles the answer at threshold `tau` (tenths): at
    /// least `tau`, with `High` confidence.
    pub fn is_decisive(&self, tau: i64) -> bool {
        self.high_confidence && self.tenths.is_some_and(|lead| lead >= tau)
    }

    /// Whether the lead is narrower than `below` tenths, nothing cited
    /// counting as no lead at all. Confidence plays no part.
    pub(super) fn is_narrow(&self, below: i64) -> bool {
        self.tenths_or_zero() < below
    }

    /// The lead in tenths, nothing cited reading as zero.
    pub(super) fn tenths_or_zero(&self) -> i64 {
        self.tenths.unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn margin(tenths: Option<i64>, high_confidence: bool) -> LexicalMargin {
        LexicalMargin {
            tenths,
            high_confidence,
        }
    }

    #[test]
    fn only_a_high_confidence_lead_at_the_threshold_is_decisive() {
        assert!(margin(Some(40), true).is_decisive(40));
        assert!(margin(Some(41), true).is_decisive(40));
        assert!(!margin(Some(39), true).is_decisive(40));
        assert!(!margin(Some(400), false).is_decisive(40));
        assert!(!margin(None, true).is_decisive(0));
        assert!(!margin(Some(-3), true).is_decisive(0));
        assert!(margin(Some(0), true).is_decisive(0));
    }

    #[test]
    fn a_lead_below_the_band_threshold_is_narrow_whatever_the_confidence() {
        assert!(margin(Some(19), true).is_narrow(20));
        assert!(!margin(Some(20), false).is_narrow(20));
        assert!(margin(Some(-5), false).is_narrow(0));
        assert!(margin(None, true).is_narrow(1), "nothing cited leads zero");
        assert!(!margin(None, true).is_narrow(0));
        assert_eq!(margin(None, false).tenths_or_zero(), 0);
    }

    #[test]
    fn confidence_is_carried_over_unchanged_tenths() {
        let read = margin(Some(12), false).with_high_confidence(true);
        assert_eq!(read, margin(Some(12), true));
    }
}
