use kmp_proto::v1beta1::MemoryConfidence;

/// What a doubt band's judge did to an unanchored core, for the answer that
/// stands on it: whether the deterministic core bore on the question and how
/// sure it was, and whether the judge promoted anything into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct JudgedCore {
    pub(super) bore: bool,
    pub(super) confidence: MemoryConfidence,
    pub(super) kept: bool,
    pub(super) promoted: bool,
}

impl JudgedCore {
    /// A promoted citation answers; otherwise only what bore before and was
    /// not vetoed away still does. A veto never makes an answer.
    pub(super) fn bears(&self) -> bool {
        self.promoted || (self.bore && self.kept)
    }

    /// Never surer than before the judge, and never `high` on a judge's word.
    pub(super) fn confidence(&self, recomputed: MemoryConfidence) -> MemoryConfidence {
        if self.promoted {
            at_most(recomputed, MemoryConfidence::Medium)
        } else {
            at_most(recomputed, self.confidence)
        }
    }
}

fn rank(confidence: MemoryConfidence) -> u8 {
    match confidence {
        MemoryConfidence::High => 3,
        MemoryConfidence::Medium => 2,
        MemoryConfidence::Low => 1,
        _ => 0,
    }
}

/// The lower of two confidences.
pub(super) fn at_most(confidence: MemoryConfidence, ceiling: MemoryConfidence) -> MemoryConfidence {
    if rank(confidence) > rank(ceiling) {
        ceiling
    } else {
        confidence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn judged(bore: bool, kept: bool, promoted: bool) -> JudgedCore {
        JudgedCore {
            bore,
            confidence: MemoryConfidence::Medium,
            kept,
            promoted,
        }
    }

    #[test]
    fn a_veto_only_takes_answers_away() {
        assert!(judged(true, true, false).bears());
        assert!(!judged(true, false, false).bears());
        assert!(
            !judged(false, true, false).bears(),
            "a veto never makes an answer"
        );
        assert!(judged(false, false, true).bears());
    }

    #[test]
    fn confidence_never_rises_and_a_promotion_is_never_high() {
        assert_eq!(
            judged(true, true, false).confidence(MemoryConfidence::High),
            MemoryConfidence::Medium
        );
        assert_eq!(
            judged(true, true, false).confidence(MemoryConfidence::Low),
            MemoryConfidence::Low
        );
        assert_eq!(
            judged(false, false, true).confidence(MemoryConfidence::High),
            MemoryConfidence::Medium
        );
        assert_eq!(
            at_most(MemoryConfidence::Unknown, MemoryConfidence::High),
            MemoryConfidence::Unknown
        );
    }
}
