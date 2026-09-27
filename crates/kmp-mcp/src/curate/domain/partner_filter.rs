/// What a review without `focus` asks of a pair that Jev's partner round
/// found, before the pair is typed. Off by default: on a large about the
/// round pairs notes written from one template (two people approving the
/// same offsite's budget), and these are the two cheap filters measured
/// against it (`docs/development/jev-evaluation.md`), neither adopted
/// without data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PartnerFilter {
    #[default]
    Off,
    /// Kept only when the two facts share a rare term: one that at most
    /// [`PartnerFilter::rare_within`] facts of the about carry.
    RareTerm,
    /// Kept only when Jev confirms the pair with a yes/no question of its
    /// own, through the verdict book.
    Confirm,
}

impl PartnerFilter {
    /// A confirmed pair has at least this probability of yes.
    pub(crate) const CONFIRM_AT: f64 = 0.5;

    /// The filter a store names (`off`, `rare_term`, `confirm`), or `None`
    /// for any other name.
    pub(crate) fn named(name: &str) -> Option<Self> {
        match name {
            "off" => Some(Self::Off),
            "rare_term" => Some(Self::RareTerm),
            "confirm" => Some(Self::Confirm),
            _ => None,
        }
    }

    /// The most facts of an about of `facts` a term may appear in and still
    /// be rare: 2 %, and never fewer than two (the pair itself).
    pub(crate) fn rare_within(facts: usize) -> usize {
        (facts * 2).div_ceil(100).max(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_names_one_of_three_filters_and_off_is_the_default() {
        assert_eq!(PartnerFilter::default(), PartnerFilter::Off);
        assert_eq!(
            PartnerFilter::named("rare_term"),
            Some(PartnerFilter::RareTerm)
        );
        assert_eq!(
            PartnerFilter::named("confirm"),
            Some(PartnerFilter::Confirm)
        );
        assert_eq!(PartnerFilter::named("off"), Some(PartnerFilter::Off));
        assert_eq!(PartnerFilter::named("idf"), None);
    }

    #[test]
    fn a_rare_term_is_carried_by_at_most_two_percent_of_the_about() {
        assert_eq!(PartnerFilter::rare_within(10), 2);
        assert_eq!(PartnerFilter::rare_within(120), 3);
        assert_eq!(PartnerFilter::rare_within(316), 7);
        assert_eq!(PartnerFilter::rare_within(512), 11);
    }
}
