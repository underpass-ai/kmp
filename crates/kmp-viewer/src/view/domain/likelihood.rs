//! How strongly a judge holds a path step.

/// A probability kept to the thousandth, so view states stay comparable and
/// a replayed intent is the same intent. The judge's own float is rounded
/// once, at the boundary; the loom never needs more resolution than it can
/// print.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Likelihood(u16);

impl Likelihood {
    /// A declared relation: the writer's word, not a guess.
    pub const CERTAIN: Self = Self(1000);

    /// Rounds a probability, clamping what is out of range or not a number.
    pub fn from_probability(probability: f64) -> Self {
        if !probability.is_finite() {
            return Self(0);
        }
        Self((probability.clamp(0.0, 1.0) * 1000.0).round() as u16)
    }

    /// The probability, to the thousandth.
    pub fn probability(self) -> f64 {
        f64::from(self.0) / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::Likelihood;

    #[test]
    fn a_probability_rounds_to_the_thousandth_and_clamps() {
        assert_eq!(Likelihood::from_probability(0.72449).probability(), 0.724);
        assert_eq!(Likelihood::from_probability(1.7), Likelihood::CERTAIN);
        assert_eq!(Likelihood::from_probability(-0.2).probability(), 0.0);
        assert_eq!(Likelihood::from_probability(f64::NAN).probability(), 0.0);
    }
}
