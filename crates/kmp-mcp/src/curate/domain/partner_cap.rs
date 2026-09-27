/// The largest about in which a review without `focus` asks Jev for the
/// partners of its orphans: a choice per orphan over every other fact of the
/// about. Past it the review names the focused review instead.
///
/// The default, 120, is Tirso's decision of 27 Sept 2026 over the measured
/// 60/120/240 comparison (`docs/development/jev-evaluation.md`). A store
/// raises it to at most 512 in `curate.json` (`partner_facts`); evaluation
/// names one through `KMP_EVAL_PARTNER_FACTS` (see `serving::environment`).
/// A choice offers at most 255 options, so an about of more than 255 facts
/// is read in windows ([`PartnerCap::windows`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PartnerCap(usize);

impl PartnerCap {
    /// Facts of an about the partner choice reads, unless the store or an
    /// evaluation says otherwise.
    pub(crate) const DEFAULT: Self = Self(120);
    /// The largest cap a store or an evaluation may name.
    pub(crate) const MAX_FACTS: usize = 512;
    /// An about of at most this many facts is read by one choice per orphan:
    /// the other facts and `none` are at most 255 options.
    pub(crate) const ONE_CHOICE_FACTS: usize = 255;
    /// Candidates of one window when the about is read in windows: with
    /// `none`, a choice of at most 240 options.
    pub(crate) const WINDOW_CANDIDATES: usize = 239;

    /// The cap `value` names: a whole number between 2 and [`Self::MAX_FACTS`];
    /// None for anything else.
    pub(crate) fn named(value: &str) -> Option<Self> {
        value
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|facts| (2..=Self::MAX_FACTS).contains(facts))
            .map(Self)
    }

    pub(crate) fn facts(self) -> usize {
        self.0
    }

    /// Whether an about of `facts` current facts gets a partner round.
    pub(crate) fn admits(self, facts: usize) -> bool {
        facts <= self.0
    }

    /// How an about of `facts` facts is offered to its orphans, as ranges
    /// over its facts in the about's order: one range when every other fact
    /// fits one choice (at most 255 facts), else the fewest windows of at
    /// most [`Self::WINDOW_CANDIDATES`], as even as possible, the longer
    /// first.
    pub(crate) fn windows(facts: usize) -> Vec<std::ops::Range<usize>> {
        if facts <= Self::ONE_CHOICE_FACTS {
            return std::iter::once(0..facts).collect();
        }
        let count = facts.div_ceil(Self::WINDOW_CANDIDATES);
        let (base, longer) = (facts / count, facts % count);
        let mut start = 0;
        (0..count)
            .map(|window| {
                let length = base + usize::from(window < longer);
                let range = start..start + length;
                start += length;
                range
            })
            .collect()
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
    fn a_cap_is_a_whole_number_of_facts_between_2_and_512() {
        assert_eq!(PartnerCap::default(), PartnerCap::DEFAULT);
        assert_eq!(PartnerCap::named("120").map(PartnerCap::facts), Some(120));
        assert_eq!(PartnerCap::named(" 240 ").map(PartnerCap::facts), Some(240));
        assert_eq!(PartnerCap::named("255").map(PartnerCap::facts), Some(255));
        assert_eq!(PartnerCap::named("512").map(PartnerCap::facts), Some(512));
        assert_eq!(PartnerCap::DEFAULT.facts(), 120);
        for refused in ["513", "1", "0", "-3", "many", ""] {
            assert_eq!(PartnerCap::named(refused), None);
        }
    }

    #[test]
    fn an_about_fits_up_to_the_cap() {
        let cap = PartnerCap::DEFAULT;
        assert!(cap.admits(120));
        assert!(!cap.admits(121));
    }

    #[test]
    fn an_about_past_one_choice_is_read_in_even_windows_of_at_most_239() {
        let whole = |facts: usize| {
            let windows = PartnerCap::windows(facts);
            windows.len() == 1 && windows[0] == (0..facts)
        };
        assert!(whole(0) && whole(118));
        assert!(whole(255), "254 facts and none");
        assert_eq!(PartnerCap::windows(256), vec![0..128, 128..256]);
        assert_eq!(PartnerCap::windows(316), vec![0..158, 158..316]);
        assert_eq!(PartnerCap::windows(512), vec![0..171, 171..342, 342..512]);
        for facts in 256..=512 {
            let windows = PartnerCap::windows(facts);
            assert_eq!(windows.first().map(|w| w.start), Some(0));
            assert_eq!(windows.last().map(|w| w.end), Some(facts));
            assert!(windows.windows(2).all(|pair| pair[0].end == pair[1].start));
            assert!(
                windows
                    .iter()
                    .all(|w| w.len() <= PartnerCap::WINDOW_CANDIDATES)
            );
        }
    }
}
