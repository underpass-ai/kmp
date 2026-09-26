/// The largest about in which a review without `focus` asks Jev for the
/// partners of its orphans: one choice per orphan over every other fact of
/// the about. Past it the review names the focused review instead.
///
/// The default is a measured constant; evaluation compares others through
/// `KMP_EVAL_PARTNER_FACTS` (see `serving::environment`). A choice offers at
/// most 255 options (the other facts and `none`), so no cap goes past 255.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PartnerCap(usize);

impl PartnerCap {
    /// Facts of an about the partner choice reads whole, unless evaluation
    /// says otherwise.
    pub(crate) const DEFAULT: Self = Self(60);
    /// A choice over `n` facts offers `n - 1` partners and `none`.
    const MAX_FACTS: usize = 255;

    /// The cap an evaluation names; the default for none, or for a value
    /// that is not a whole number between 2 and 255.
    pub(crate) fn from_eval(value: Option<&str>) -> Self {
        value
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|facts| (2..=Self::MAX_FACTS).contains(facts))
            .map_or(Self::DEFAULT, Self)
    }

    pub(crate) fn facts(self) -> usize {
        self.0
    }

    /// Whether an about of `facts` current facts fits one partner choice.
    pub(crate) fn admits(self, facts: usize) -> bool {
        facts <= self.0
    }
}

impl Default for PartnerCap {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluation_names_a_cap_within_one_choice_and_anything_else_is_the_default() {
        assert_eq!(PartnerCap::default(), PartnerCap::DEFAULT);
        assert_eq!(PartnerCap::from_eval(None), PartnerCap::DEFAULT);
        assert_eq!(PartnerCap::from_eval(Some("120")).facts(), 120);
        assert_eq!(PartnerCap::from_eval(Some(" 240 ")).facts(), 240);
        assert_eq!(PartnerCap::from_eval(Some("255")).facts(), 255);
        for refused in ["256", "1", "0", "-3", "many", ""] {
            assert_eq!(PartnerCap::from_eval(Some(refused)), PartnerCap::DEFAULT);
        }
    }

    #[test]
    fn an_about_fits_up_to_the_cap() {
        let cap = PartnerCap::from_eval(Some("120"));
        assert!(cap.admits(120));
        assert!(!cap.admits(121));
    }
}
