/// How decisively the lexical ranker's first candidate leads an Ask
/// (DESIGN L4 4c, after AgentIR 2605.25092): the content score of its first
/// eligible candidate minus the second's, in whole tenths of a point, and
/// whether the answer it would give is of `High` confidence.
///
/// A remote judge adds nothing to an answer the text already settles: when
/// the lead is at least the store's threshold and the confidence is high,
/// the Ask reads the lexical order and sends no request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexicalMargin {
    /// `None` when no candidate was eligible. A lone eligible candidate
    /// leads by its whole score.
    pub tenths: Option<i64>,
    pub high_confidence: bool,
}

impl LexicalMargin {
    /// Whether the lead settles the answer at threshold `tau` (tenths).
    pub fn is_decisive(&self, tau: i64) -> bool {
        self.high_confidence && self.tenths.is_some_and(|lead| lead >= tau)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_high_confidence_lead_at_the_threshold_is_decisive() {
        let margin = |tenths, high_confidence| LexicalMargin {
            tenths,
            high_confidence,
        };
        assert!(margin(Some(40), true).is_decisive(40));
        assert!(margin(Some(41), true).is_decisive(40));
        assert!(!margin(Some(39), true).is_decisive(40));
        assert!(!margin(Some(400), false).is_decisive(40));
        assert!(!margin(None, true).is_decisive(0));
        assert!(!margin(Some(-3), true).is_decisive(0));
        assert!(margin(Some(0), true).is_decisive(0));
    }
}
