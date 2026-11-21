/// How the cross-language judge reads Jev's answer to "does `passage` state
/// the answer to `question`?", and what the answer does with it. Jev makes
/// the judgement; this table makes the decision (TypeSafe: judgments from
/// Jev, decisions in code).
///
/// A Noul is read in three bands: at or above the line asked for it says
/// yes; at or below [`Self::NO_AT`] it says no; in between the question is
/// not settled, which gets its own row:
///
/// | Reading of `p` = P(the passage states it) | Translated citation (veto) | Rescue candidate |
/// |:--|:--|:--|
/// | `States`: `p` ≥ the line (`keep_at` 0.7; `rescue_at` 0.9), and for a rescue the companion too | kept | cited, the likeliest one only |
/// | `Unsettled`: 0.3 < `p` < the line | leaves the core: an answer resting on a translation is not vouched for | not cited |
/// | `Absent`: `p` ≤ 0.3 | leaves the core | not cited |
/// | no answer (late, failed, malformed) | today's answer stands, with a warning | today's answer stands, with a warning |
///
/// A judgement that did not come back is never read as a no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StatedReading {
    States,
    Unsettled,
    Absent,
}

impl StatedReading {
    /// At or below this many thousandths a Noul reads as no.
    pub(super) const NO_AT: u16 = 300;

    /// The reading of `states` thousandths against the line `yes_at`.
    pub(super) fn of(states: u16, yes_at: u16) -> Self {
        if states >= yes_at {
            Self::States
        } else if states > Self::NO_AT {
            Self::Unsettled
        } else {
            Self::Absent
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StatedReading;

    #[test]
    fn three_bands_with_the_line_inclusive() {
        assert_eq!(StatedReading::of(700, 700), StatedReading::States);
        assert_eq!(StatedReading::of(699, 700), StatedReading::Unsettled);
        assert_eq!(StatedReading::of(301, 700), StatedReading::Unsettled);
        assert_eq!(StatedReading::of(300, 700), StatedReading::Absent);
        assert_eq!(StatedReading::of(850, 900), StatedReading::Unsettled);
    }
}
